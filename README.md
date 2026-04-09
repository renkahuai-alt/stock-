# new_stock

这是一个可以自定义美股板块的工具。

界面：
<img width="1439" height="900" alt="image" src="https://github.com/user-attachments/assets/7b3f1bf9-d172-40b0-8096-c72ceec3bbfc" />

可以查看你自己定义的板块、板块内的个股和四个具有代表性的指数，支持盘中低频更新当天日 K。

<img width="721" height="559" alt="image" src="https://github.com/user-attachments/assets/d563daca-a466-467d-9ce1-148c4417ad91" />

需要输入你的长桥 API 获取数据，板块自定义也在设置页面。

说明：
- 旧项目代码不复用运行时实现，只作为行为、UI 和测试口径对照组
- 当前本地主开发平台仍以 `macOS` 为主
- 已补充 Windows 安装包构建链路，见 `.github/workflows/windows-build.yml`
- 前端与后端职责已经在需求文档中拆开，实施时不得跨边界扩展

运行与数据：
- 默认数据库不再落在源码目录，发布版本会使用系统用户数据目录
- 凭证存储按平台走系统安全存储：
  - `macOS`: Keychain
  - `Windows`: Credential Manager
