use std::fs;
use std::path::PathBuf;

use new_stock_lib::jobs::board_build;
use new_stock_lib::models::{GetChartPayload, SaveBoardPayload, SaveCredentialsPayload};
use new_stock_lib::repositories::Database;
use new_stock_lib::secret_store;
use new_stock_lib::services::{
    AppRuntime, BoardService, BootstrapService, ChartService, SyncService,
};
use uuid::Uuid;

fn temp_db_path(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "new-stock-real-smoke-{test_name}-{}.sqlite3",
        Uuid::new_v4()
    ));
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    path
}

fn load_real_credentials() -> SaveCredentialsPayload {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let raw = fs::read_to_string(manifest_dir.join("../docs/api_mima"))
        .expect("docs/api_mima should exist for manual real smoke");
    let lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    assert!(
        lines.len() >= 6,
        "docs/api_mima should contain App Key / App Secret / Access Token"
    );

    SaveCredentialsPayload {
        app_key: lines[1].to_string(),
        app_secret: lines[3].to_string(),
        access_token: lines[5].to_string(),
    }
}

fn chart_request(board_id: &str) -> GetChartPayload {
    GetChartPayload {
        target_type: "board".to_string(),
        target_id: board_id.to_string(),
        granularity: Some("day".to_string()),
        range: Some("1y".to_string()),
        board_algorithm: Some("market_cap_weight_v1".to_string()),
    }
}

fn index_chart_request(target_id: &str) -> GetChartPayload {
    GetChartPayload {
        target_type: "index".to_string(),
        target_id: target_id.to_string(),
        granularity: Some("day".to_string()),
        range: Some("1y".to_string()),
        board_algorithm: None,
    }
}

#[tokio::test]
#[ignore = "manual smoke for real Longbridge credentials"]
async fn manual_real_save_board_background_build_smoke() {
    secret_store::save_credentials(&load_real_credentials())
        .expect("real credentials should save into keychain");

    let database = Database::at(temp_db_path("save-board-background"));
    database.bootstrap().expect("bootstrap should succeed");

    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "存储芯片".to_string(),
            members: vec![
                "MU".to_string(),
                "SNDK".to_string(),
                "WDC".to_string(),
                "STX".to_string(),
            ],
            composition_algorithm: "market_cap_weight_v1".to_string(),
        })
        .expect("missing local symbols should queue background build");

    assert!(response.background_sync_started);
    assert_eq!(response.build_status, "queued");
    assert_eq!(response.build_phase, "queued");

    board_build::run(
        database.clone(),
        AppRuntime::default(),
        &response.board_id,
        None,
    )
    .await
    .expect("real provider board build should succeed");

    let status = BoardService::new(database.clone())
        .get_build_status(&response.board_id)
        .expect("build status should load");
    assert_eq!(status.build_status, "succeeded");
    assert_eq!(status.build_phase, "completed");
    assert_eq!(status.build_total, 4);
    assert_eq!(status.build_completed + status.build_failed, 4);

    let chart = ChartService::with_runtime(database.clone(), AppRuntime::default())
        .get_chart(chart_request(&response.board_id))
        .expect("board chart should load");
    assert!(
        !chart.bars.is_empty(),
        "real smoke should persist board bars"
    );

    let members = vec![
        "MU".to_string(),
        "SNDK".to_string(),
        "WDC".to_string(),
        "STX".to_string(),
    ];
    let symbols = database
        .list_symbols_by_ids(&members)
        .expect("symbol query should succeed");
    assert!(
        !symbols.is_empty(),
        "real smoke should backfill symbol rows"
    );
}

#[tokio::test]
#[ignore = "manual smoke for real Longbridge credentials"]
async fn manual_real_index_chart_smoke() {
    secret_store::save_credentials(&load_real_credentials())
        .expect("real credentials should save into keychain");

    let database = Database::at(temp_db_path("index-chart"));
    database.bootstrap().expect("bootstrap should succeed");

    let bootstrap = BootstrapService::new(database.clone())
        .load()
        .expect("bootstrap should load");
    let index_ids = bootstrap
        .indexes
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(index_ids, vec!["DJI", "IXIC", "GSPC", "RUT"]);

    let sync = SyncService::new(database.clone(), AppRuntime::default())
        .run("manual")
        .await
        .expect("real index sync should succeed");
    assert_eq!(sync.status, "ready");

    let chart_service = ChartService::with_runtime(database, AppRuntime::default());
    for index_id in ["DJI", "IXIC", "GSPC", "RUT"] {
        let chart = chart_service
            .get_chart(index_chart_request(index_id))
            .expect("real index chart should load");
        assert!(
            !chart.bars.is_empty(),
            "real smoke should return bars for {index_id}"
        );
        assert_eq!(
            chart.meta.provider_kind.as_deref(),
            match index_id {
                "GSPC" | "RUT" => Some("longbridge_proxy_etf"),
                _ => Some("longbridge"),
            },
            "index chart providerKind should reflect the real index path"
        );
        assert_eq!(
            chart.meta.value_mode, None,
            "index chart should no longer expose fixture value mode"
        );
    }
}
