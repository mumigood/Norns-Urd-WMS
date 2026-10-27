Urd-WMS Windows 11 整合版：egui 桌面 + 網頁，共用原有資料
======================================================

套件包含 Urd-WMS.exe、urdwms-server.exe、Caddy 2、預先建置的 www 靜態網頁。
Windows 11 x64 / 8GB Surface 可原生執行，不需要 WSL、Python、Node.js 或套件管理器。
ARM64 Surface 目前需 x64 模擬，未原生驗證。原生桌面需要 OpenGL 2.0 以上。

一鍵啟動
--------
1. 首次升級前，先關閉正在執行的舊版 Urd-WMS。
2. 解壓整個套件到固定資料夾，例如 C:\Urd-WMS。
3. 雙擊 Start-WMS.cmd。它先啟動 API，等資料庫備份／遷移完成且健康檢查成功，
   再啟動 Caddy 與 egui。所有程式使用同一個資料目錄。
4. 預設監聽 0.0.0.0:8080；本機開啟 http://localhost:8080，其他電腦開啟
   http://主機區網IP:8080。既有帳號與密碼直接沿用。
   只有全新空資料庫才建立 admin / urdwms-admin，登入後必須立刻改密碼。
5. 關閉 egui 視窗，網頁仍繼續運作。停止整個系統時，在啟動視窗按 Ctrl+C；
   它會清理由本次啟動的桌面、API 與 Caddy。先停止再關閉啟動視窗。
   請勿用工作管理員強制終止啟動腳本，這會略過清理。

Start-WMS.cmd 使用 Windows PowerShell 5.1，執行原則 Bypass 只限該次程序，
不修改系統設定。組織群組原則仍優先，若禁止腳本請由管理員依政策處理。
啟動不需管理員身分，不自動修改防火牆、DNS 或網路轉送。
內附 caddy.exe 優先使用，若自行組裝未附此檔，則從 PATH 尋找。
Caddy 版本與官方來源／SHA512 見 Caddy-version.txt，授權見 LICENSE-Caddy.txt。

原有 egui 資料如何沿用
---------------------
預設資料目錄仍是 %LOCALAPPDATA%\UrdWMS，不再另外建立 UrdWMS-Web 空資料庫。
URDWMS_DATA 環境變數可覆寫此目錄。曾用自訂 --data-dir 的使用者，請把同一路徑
傳入啟動腳本的 -DataDir，或設定 URDWMS_DATA。不要把資料目錄設成程式所在目錄。

第一次開啟尚未標記版本的舊資料庫，程式會先檢查完整性，以 SQLite 備份機制
把資料庫（包含已提交的 WAL 內容）備份至：
  DataDir\backups\urdwms-v0-<UTC時間>-<隨機值>.db
然後在交易內升級資料表並記錄 schema 版本。既有帳號、密碼雜湊、批次、領用、
異動軌跡與設定會保留；DataDir\uploads 照片原地保留。這是同一台主機的原地升級，
不需要匯出／匯入或複製一份空資料庫。同版本再次啟動不會每次重做備份。
遇到比程式更新的資料庫版本或遷移失敗，啟動會中止，不會偷偷重建或重置資料。
保留日誌和備份交由管理員確認，不要直接刪除資料庫重試。

桌面透過共享 SQLite 服務層操作，API 使用獨立連線讀寫同一個檔案。
egui 偵測外部提交後更新資料與登入狀態；未儲存表單不會被重置。
瀏覽器重新整理／重新載入會讀到同一份最新資料。遠端裝置用瀏覽器連到這台主機，
egui 本身不是遠端 API 用戶端。不要讓多台主機把 SQLite 放在共用網路磁碟上。

自訂啟動
--------
在 PowerShell 執行（路徑含空白可正常使用）：
  & 'C:\Urd-WMS\Run-WMS.ps1' -DataDir 'D:\WMS Data'
  .\Run-WMS.ps1 -WebOnly
  .\Run-WMS.ps1 -ApiPort 8072 -WebAddress 'http://:8081'
  .\Run-WMS.ps1 -Demo -DataDir 'C:\UrdWMS-demo'

-WebOnly 只啟動 API/Caddy；Run-Web.ps1 是同一個啟動流程的相容入口。
-Demo 只用在獨立測試資料夾，正式資料不應使用。
示範帳號 peter / peter0821、kuo / urdwms2026、operator / urdwms2026。

參數優先於環境變數，參數 / 環境變數 / 預設如下：
  -DataDir      URDWMS_DATA          %LOCALAPPDATA%\UrdWMS
                URDWMS_DATA_DIR      舊啟動器相容別名，URDWMS_DATA 未設定才使用
  -ApiPort      URDWMS_API_PORT      8071
  -WebAddress   URDWMS_WEB_ADDRESS   http://:8080
  -WebBind      URDWMS_WEB_BIND      0.0.0.0（所有 IPv4 網路介面）
  -Caddy                            套件內 caddy.exe，其次 PATH
  -StartupTimeoutSeconds            600（大型舊資料庫備份可增加，最多 3600）

API 僅監聽 127.0.0.1。Caddy 的 /api 和 /uploads 轉送 API，其他路徑讀取 www。
照片仍經 API 登入驗證。Web 預設接受所有 IPv4 網路介面的 8080 連線，
不需另加啟動參數。防火牆、網路轉送、DNS 與憑證由網管設定，程式不代管。

備份、更新與日誌
----------------
日常備份先 Ctrl+C 停止整合啟動器，也停止其他獨立開啟的 egui，再複製完整 DataDir，
包括 urdwms.db、uploads、backups 與其他資料。升級前也建議保留完整資料夾備份；
自動遷移的 .db 備份只涵蓋資料庫，沒有另外複製 uploads。
升級程式時保留 DataDir，替換程式與 www 即可，不要覆寫資料庫與照片。

DataDir 的 api.stdout.log、api.stderr.log、web.stdout.log、web.stderr.log 每次啟動
會覆寫，遇到問題請先保存。web-launcher.lock 防止同資料目錄重複啟動，退出時會釋放，
鎖檔留存是正常現象。埠被占用時先停止原服務，或指定另一個埠。
本套件是前景啟動器，沒有安裝 Windows 服務；主機重新登入後需手動啟動。

開發建置（僅開發機 / CI 需要 Rust、Node 與 pnpm）
------------------------------------------------
  cargo build --release -p urdwms --bin urdwms
  cargo build --release -p urdwms --no-default-features --features server --bin urdwms-server
  cd frontend
  pnpm install --frozen-lockfile
  pnpm build

將桌面執行檔命名為 Urd-WMS.exe，API 為 urdwms-server.exe，frontend/out 放入 www。
Caddy 由 packaging/Get-Caddy.ps1 從官方固定版本下載並核對 SHA512，只在建置時執行。
非原生 MSVC 建置若需要額外 runtime DLL，必須同包附上對應 DLL 和授權。
CI 會建置 Windows 程式、檢查 PowerShell 語法與 API health；實際 Windows 啟動／
關閉流程、相機、TLS 與 Surface 效能仍需原生 Windows 現場驗證。
