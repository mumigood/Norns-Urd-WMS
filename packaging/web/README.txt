Urd-WMS Windows 11 網頁部署（x64）
================================

這個資料夾包含 Rust API、預先建置的靜態網頁與 Caddy 設定。
執行時不需要 WSL、Python、Node.js、npm 或 pnpm。Windows PowerShell 5.1 即可。
適用 x64 Windows 11（例如 8GB Surface）；ARM64 Surface 目前需 x64 模擬，未原生驗證。

準備
----
1. 解壓整個套件到固定資料夾，例如 C:\Urd-WMS-Web。
2. 另外取得 Windows x64 Caddy 2（https://caddyserver.com/download），
   將 caddy.exe 放進 PATH，或在啟動時用 -Caddy 指定完整路徑。
   本套件不下載或暗中捆綁 Caddy；請依組織政策驗證該執行檔。
3. 開啟 PowerShell，執行（路徑依實際解壓位置修改）：

   & 'C:\Urd-WMS-Web\Run-Web.ps1' -Caddy 'C:\Tools\caddy.exe'

   若公司執行原則禁止腳本，交由管理員依政策允許已審閱的腳本。
   不需要系統管理員身分，也不會變更防火牆或網路轉送。
4. 開啟 http://localhost:8080 。全新資料庫的帳號 admin / 密碼 urdwms-admin，
   登入後必須立即更改密碼，再建立其他使用者。
5. 保持 PowerShell 視窗開啟。按 Ctrl+C 停止 API 和 Caddy，再關閉視窗。
   啟動失敗或任一子程式退出時，啟動腳本會清理另一個子程式。
   請勿以工作管理員強制終止啟動腳本；這會略過清理。

設定
----
參數優先於環境變數，以下列出參數 / 環境變數 / 預設：
  -DataDir     URDWMS_DATA_DIR      %LOCALAPPDATA%\UrdWMS-Web
  -ApiPort     URDWMS_API_PORT      8071
  -WebAddress  URDWMS_WEB_ADDRESS   http://localhost:8080
  -WebBind     URDWMS_WEB_BIND      127.0.0.1（僅本機）
  -Caddy                           caddy.exe（PATH）
  -Demo                            不啟用；只能用在獨立測試資料夾

範例（路徑含空白也可）：
  .\Run-Web.ps1 -DataDir 'D:\WMS Data' -ApiPort 8072 -WebAddress 'http://localhost:8081'
  .\Run-Web.ps1 -Demo -DataDir 'C:\UrdWMS-Web-demo'
示範帳號 peter / peter0821、kuo / urdwms2026、operator / urdwms2026。

API 固定只監聽 127.0.0.1。Caddy 把同來源的 /api、/uploads 轉送 API，
其餘路徑只讀 www 靜態檔案；照片仍經 API 登入驗證。
預設 localhost 網址只供這台主機使用。區網部署可設定 -WebAddress 'http://:8080' -WebBind '0.0.0.0'，
其他裝置使用 http://主機IP:8080 。HTTP 會明文傳送登入資料，正式部署應設定
組織的 HTTPS 網域，例如 -WebAddress 'wms.example.com' -WebBind '0.0.0.0'，並讓管理員配置 DNS、
憑證、必要防火牆規則與網路轉送。Caddy 的自動 HTTPS 需要符合其憑證簽發條件。
這些網路設定不由程式自動修改。HTTPS 對瀏覽器相機權限也有影響。

資料與備份
----------
所有瀏覽器都操作這台 API 主機的同一份 SQLite 資料庫與 uploads 照片。
桌面 Urd-WMS.exe 是本機獨立 egui 程式，不是遠端 API 用戶端；預設使用另一個
%LOCALAPPDATA%\UrdWMS 資料夾。要遠端操作，請使用瀏覽器。
不要把 SQLite 放在網路磁碟上，也不要讓多台 API 主機共用資料庫檔案。

先 Ctrl+C 停止服務，再備份完整 DataDir（urdwms.db、uploads 及其他資料）。
從桌面或舊 Python 版搬移時，先停止舊程式並備份，複製資料庫和 uploads 到新的
DataDir 後再啟動 API；不要只換網頁而遺漏照片。既有帳號不會被初始 admin 覆寫。
升級前先停止服務並備份 DataDir，再替換程式與 www；保留資料夾不動。

啟動日誌位於 DataDir：api.stdout.log、api.stderr.log、web.stdout.log、web.stderr.log。
每次啟動覆寫這四個啟動日誌；遇到問題請先保存。web-launcher.lock 防止重複啟動，
檔案留存是正常現象，鎖會在退出時釋放。若埠被占用，改用其他埠或停止原服務。
這是前景啟動範例，未安裝 Windows 服務；登入後需手動啟動。

開發與建置（只在開發機 / CI 需要 Node 與 Rust）
------------------------------------------------
  cargo build --release -p urdwms --no-default-features --features server --bin urdwms-server
  cd frontend
  pnpm install --frozen-lockfile
  pnpm build

把 target/release/urdwms-server.exe 放在本檔旁，frontend/out 的內容放進 www。
靜態網站不需要 next start。不要把 .next 或原始碼當作可部署靜態輸出。
CI 在 Windows 建置並測試 API health 與資料庫建立；實際防火牆、TLS、相機、
Surface 效能與關閉視窗行為仍需在部署現場確認。
