//! Native Windows/Linux headless API. Static pages are served separately.
use std::{net::SocketAddr, path::PathBuf};
use urdwms::{applog, seed, service::Store, web};

const HELP: &str = "Urd-WMS API\n\nurdwms-server [--listen 127.0.0.1:8071] [--data-dir PATH] [--demo]\n\n--listen     HTTP 監聽位址（預設 127.0.0.1:8071）\n--data-dir   資料目錄（預設 URDWMS_DATA 或使用者 UrdWMS-Web 目錄）\n--demo       空資料庫時建立示範資料\n--version    顯示版本\n";

#[derive(Debug)]
struct Args {
    listen: SocketAddr,
    data_dir: PathBuf,
    demo: bool,
}
fn default_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("URDWMS_DATA") {
        return dir.into();
    }
    #[cfg(windows)]
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(dir).join("UrdWMS-Web");
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".local/share/urdwms-web"))
        .unwrap_or_else(|| PathBuf::from("urdwms-web-data"))
}
fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = args;
    let mut result = Args { listen: "127.0.0.1:8071".parse().unwrap(), data_dir: default_data_dir(), demo: false };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => {
                result.listen =
                    args.next().ok_or("--listen 需要位址")?.parse().map_err(|_| "--listen 必須是 IP:port")?
            }
            "--data-dir" => result.data_dir = args.next().ok_or("--data-dir 需要路徑")?.into(),
            "--demo" => result.demo = true,
            _ => return Err(format!("不認得的參數：{arg}\n{HELP}")),
        }
    }
    Ok(result)
}
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("urdwms-server {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let result = async {
        let args = parse(args.into_iter()).map_err(std::io::Error::other)?;
        let store = Store::open(&args.data_dir)?;
        applog::init(&args.data_dir);
        if args.demo {
            seed::demo(&store.conn)?;
        }
        if seed::first_run(&store.conn)?.is_some() {
            println!("首次啟動：請使用 admin / {} 登入並立即修改密碼。", seed::DEFAULT_ADMIN_PASSWORD);
        }
        drop(store);
        let listener = tokio::net::TcpListener::bind(args.listen).await?;
        println!("Urd-WMS API http://{} 資料目錄：{}", listener.local_addr()?, args.data_dir.display());
        applog::info(format!("API 啟動 {}", listener.local_addr()?));
        axum::serve(listener, web::router(args.data_dir))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    if let Err(error) = result {
        applog::error(error.to_string());
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_explicit_listen_and_preserves_space_in_data_path() {
        let a =
            parse(["--listen", "0.0.0.0:8090", "--data-dir", "C:\\WMS Data", "--demo"].into_iter().map(str::to_string))
                .unwrap();
        assert_eq!(a.listen, "0.0.0.0:8090".parse::<SocketAddr>().unwrap());
        assert_eq!(a.data_dir, PathBuf::from("C:\\WMS Data"));
        assert!(a.demo);
        assert!(parse(["--listen", "invalid"].into_iter().map(str::to_string)).is_err());
        assert!(parse(["--data-dir"].into_iter().map(str::to_string)).is_err());
    }
}
