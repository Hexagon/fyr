//! End-to-end HTTPS tests: the real router and `serve` loop on 127.0.0.1:0
//! with a generated certificate, exercised by a rustls client that trusts
//! only that certificate.

use crate::{auth, create_router, messaging, serve, tls, AppState};
use crate::{ai::ModelManager, mbtiles::MbtilesPool, settings::SettingsManager};
use downloader::DownloadManager;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use types::{Config, TlsMode};

struct Running {
    addr: SocketAddr,
    redirect_addr: Option<SocketAddr>,
    data_dir: PathBuf,
    fingerprint: String,
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

async fn start(mode: TlsMode, password: Option<&str>, redirect: bool, data_dir: PathBuf) -> Running {
    let mut config = Config::default_with_data_dir(&data_dir);
    config.initialize_directories().expect("initialize directories");
    config.auth.admin_password = password.map(String::from);
    config.server.host = "127.0.0.1".into();
    config.server.tls.mode = mode;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    config.server.port = addr.port();
    let redirect_listener = redirect.then(|| std::net::TcpListener::bind("127.0.0.1:0").unwrap());
    let redirect_addr = redirect_listener.as_ref().map(|l| l.local_addr().unwrap());
    config.server.tls.http_redirect_port = redirect_addr.map(|a| a.port());

    let setup = tls::prepare(&config).expect("prepare TLS").expect("TLS enabled");
    config.server.tls.fingerprint_sha256 = Some(setup.fingerprint_sha256.clone());
    let config = Arc::new(config);
    let downloads = Arc::new(DownloadManager::new(&data_dir));
    let state = AppState {
        config: Arc::clone(&config),
        static_dir: data_dir.join("static"),
        download_manager: Arc::clone(&downloads),
        model_manager: Arc::new(ModelManager::new(Arc::clone(&config))),
        settings_manager: Arc::new(SettingsManager::new(&data_dir)),
        auth_manager: Arc::new(auth::AuthManager::new()),
        mbtiles_pool: Arc::new(MbtilesPool::new()),
        messaging: Arc::new(messaging::Messaging::open(&config).expect("open messages db")),
        fulltext: Arc::new(crate::fulltext::FulltextIndexer::from_config(&config)),
        places: Arc::new(crate::places::PlaceIndexer::from_config(&config)),
    };
    let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(serve(
        create_router(state),
        listener,
        Some(setup.server_config),
        redirect_listener.map(|l| (l, addr.port())),
        async move {
            let _ = shutdown_rx.await;
        },
        downloads,
    ));
    Running {
        addr,
        redirect_addr,
        data_dir,
        fingerprint: setup.fingerprint_sha256,
        shutdown,
        task,
    }
}

fn client_trusting(cert_pem: &[u8], localhost: SocketAddr) -> reqwest::Client {
    reqwest::Client::builder()
        .use_rustls_tls()
        .tls_built_in_root_certs(false)
        .add_root_certificate(reqwest::Certificate::from_pem(cert_pem).unwrap())
        .redirect(reqwest::redirect::Policy::none())
        .resolve("localhost", localhost)
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("fyr-tls-it-{tag}-{}", uuid::Uuid::new_v4()))
}

async fn stop(running: Running) -> Duration {
    let started = Instant::now();
    running.shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(20), running.task)
        .await
        .expect("graceful shutdown completes")
        .expect("server task joins")
        .expect("server exits cleanly");
    let _ = std::fs::remove_dir_all(&running.data_dir);
    started.elapsed()
}

#[tokio::test]
async fn self_signed_https_end_to_end() {
    let running = start(TlsMode::SelfSigned, Some("secret"), true, temp_dir("self")).await;
    let cert_pem = std::fs::read(running.data_dir.join("tls/cert.pem")).unwrap();
    let client = client_trusting(&cert_pem, running.addr);
    let base = format!("https://127.0.0.1:{}", running.addr.port());

    // Status over TLS (IP SAN); anonymous callers in password mode see the
    // mode but not the fingerprint.
    let response = client.get(format!("{base}/api/status")).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert!(response.headers().get("strict-transport-security").is_none());
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["tls"]["enabled"], true);
    assert_eq!(body["tls"]["mode"], "self-signed");
    assert!(body["tls"].get("fingerprint_sha256").is_none());

    // DNS SAN: localhost is covered too.
    let response = client
        .get(format!("https://localhost:{}/api/auth/status", running.addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Login sets a Secure session cookie (ConnectInfo works over TLS, or the
    // login handler would fail to extract the peer address).
    let response = client
        .post(format!("{base}/api/auth/login"))
        .json(&serde_json::json!({ "password": "secret" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let cookie = response.headers()["set-cookie"].to_str().unwrap().to_string();
    assert!(cookie.contains("HttpOnly") && cookie.contains("; Secure"), "{cookie}");
    let session = cookie.split(';').next().unwrap().to_string();

    // Admins see the fingerprint, matching the startup value.
    let body: serde_json::Value = client
        .get(format!("{base}/api/status"))
        .header("cookie", &session)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["tls"]["fingerprint_sha256"], running.fingerprint.as_str());

    // Plain HTTP on the redirect port answers 308 to the HTTPS port.
    let redirect = running.redirect_addr.unwrap();
    let response = client
        .get(format!("http://127.0.0.1:{}/maps?x=1", redirect.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 308);
    assert_eq!(
        response.headers()["location"],
        format!("https://127.0.0.1:{}/maps?x=1", running.addr.port()).as_str()
    );

    // A client that does not trust the certificate is refused.
    let untrusting = reqwest::Client::builder()
        .use_rustls_tls()
        .tls_built_in_root_certs(false)
        .build()
        .unwrap();
    assert!(untrusting.get(format!("{base}/api/status")).send().await.is_err());

    // A long-lived SSE stream must not block graceful shutdown beyond the
    // drain timeout.
    let stream = client
        .get(format!("{base}/api/channels/public/stream"))
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status(), 200);
    let elapsed = stop(running).await;
    assert!(elapsed < Duration::from_secs(15), "shutdown took {elapsed:?}");
    drop(stream);
}

#[tokio::test]
async fn provided_certificate_sends_hsts() {
    // Use a generated certificate as the "provided" one.
    let cert_dir = temp_dir("provided-cert");
    let names = ["localhost", "127.0.0.1"].iter().map(|s| s.to_string()).collect();
    tls::ensure_self_signed(&cert_dir, &names, 1_790_000_000).unwrap();
    let mode = TlsMode::Provided {
        cert_path: cert_dir.join("cert.pem"),
        key_path: cert_dir.join("key.pem"),
    };
    let running = start(mode, None, false, temp_dir("provided")).await;
    let cert_pem = std::fs::read(cert_dir.join("cert.pem")).unwrap();
    let client = client_trusting(&cert_pem, running.addr);

    let response = client
        .get(format!("https://127.0.0.1:{}/api/status", running.addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["strict-transport-security"], tls::HSTS_VALUE);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["tls"]["mode"], "provided");
    // Open mode: everyone may see system details, including the fingerprint.
    assert_eq!(body["tls"]["fingerprint_sha256"], running.fingerprint.as_str());
    // No self-signed material is generated in provided mode.
    assert!(!running.data_dir.join("tls").exists());

    stop(running).await;
    let _ = std::fs::remove_dir_all(cert_dir);
}

#[tokio::test]
async fn unreadable_or_missing_certificate_fails_fast() {
    let data_dir = temp_dir("missing");
    let mut config = Config::default_with_data_dir(&data_dir);
    config.server.tls.mode = TlsMode::Provided {
        cert_path: data_dir.join("nope.pem"),
        key_path: data_dir.join("nope.key"),
    };
    let error = format!("{:#}", tls::prepare(&config).err().expect("must fail"));
    assert!(error.contains("FYR_TLS_CERT") && error.contains("nope.pem"), "{error}");
}
