use anyhow::{Context, bail};
use template_application::auth::Scope;
use template_infrastructure::Infrastructure;
use template_web::auth::JwtAuth;
use template_web::config::{AppSettings, Config};
use template_web::{AppState, app};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // `template-web issue-token <service> <scope>... [--ttl-days N]` — выпустить JWT для микросервиса
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("issue-token") {
        return issue_token(&args[1..]);
    }

    let config = Config::from_env()?;

    // `template-web healthcheck` — для HEALTHCHECK в distroless-образе (там нет wget/curl)
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck(&config).await;
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,sea_orm=warn")),
        )
        .init();

    let infrastructure = Infrastructure::init(&config.infrastructure())
        .await
        .context("failed to initialise the database")?;
    let app = app(AppState::new(infrastructure, &config.app));

    let listener = TcpListener::bind(config.listen_addr).await?;
    tracing::info!(
        "listening on http://{}, docs on /scalar",
        listener.local_addr()?
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

async fn healthcheck(config: &Config) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect(("127.0.0.1", config.listen_addr.port())).await?;
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    anyhow::ensure!(
        response.starts_with("HTTP/1.1 200"),
        "unhealthy: {}",
        response.lines().next().unwrap_or("")
    );
    Ok(())
}

const ISSUE_TOKEN_USAGE: &str = "usage: template-web issue-token <service> <scope>... [--ttl-days N]\n\
     scopes: codes:write codes:read codes:read:any; default TTL 365 days";

fn issue_token(args: &[String]) -> anyhow::Result<()> {
    let mut service = None;
    let mut scopes = Vec::new();
    let mut ttl_days: i64 = 365;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ttl-days" => {
                let value = args.next().context(ISSUE_TOKEN_USAGE)?;
                ttl_days = value
                    .parse()
                    .with_context(|| format!("invalid --ttl-days: {value}"))?;
            }
            "-h" | "--help" => {
                println!("{ISSUE_TOKEN_USAGE}");
                return Ok(());
            }
            value if service.is_none() => service = Some(value.to_owned()),
            value => scopes.push(value.parse::<Scope>().map_err(anyhow::Error::msg)?),
        }
    }
    let Some(service) = service else {
        bail!(ISSUE_TOKEN_USAGE);
    };
    if template_domain::qr_code::ServiceName::parse(&service).is_err() {
        bail!("invalid service name {service:?}: lowercase latin letters, digits, '-' or '_'");
    }
    if ttl_days <= 0 {
        bail!("--ttl-days must be positive");
    }

    let settings = AppSettings::from_env()?;
    let token = JwtAuth::new(&settings.jwt_secret).issue(
        &service,
        &scopes,
        chrono::Duration::days(ttl_days),
    )?;
    println!("{token}");
    Ok(())
}
