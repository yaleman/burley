//! the listener and magic

use crate::{CacheEntry, DataStore, cli::Cli, error::BurleyError, is_cacheable_response};
use chrono::Utc;
use rama::{
    Context, Layer, Service,
    error::{ErrorContext, OpaqueError},
    http::{
        Body, HeaderMap, Method, Request, Response, StatusCode, Uri,
        client::EasyHttpWebClient,
        dep::http_body_util::BodyExt,
        header::{self, HeaderName, HeaderValue},
        layer::{
            remove_header::{RemoveRequestHeaderLayer, RemoveResponseHeaderLayer},
            trace::TraceLayer,
            upgrade::{UpgradeLayer, Upgraded},
        },
        matcher::MethodMatcher,
        server::HttpServer,
        service::web::response::IntoResponse,
    },
    net::{
        conn::is_connection_error,
        http::RequestContext,
        stream::{SocketInfo, layer::http::BodyLimitLayer},
    },
    rt::Executor,
    service::service_fn,
    tcp::{client::default_tcp_connect, server::TcpListener},
    tls::rustls::{
        dep::{
            pemfile,
            pki_types::{CertificateDer, PrivateKeyDer},
        },
        server::{TlsAcceptorData, TlsAcceptorDataBuilder, TlsAcceptorLayer},
    },
};
use serde_with::{DisplayFromStr, serde_as};
use std::{convert::Infallible, io::BufReader, path::Path, sync::Arc};
use tempfile::tempdir;
use tracing::{debug, error, info};

const BODY_LIMIT: usize = 1024 * 1024 * 1024;
const CACHE_MAX_SIZE: usize = 1024 * 1024 * 20;
const CACHE_HEADER: &str = "x-burley-cache";

#[derive(Clone)]
struct ProxyState {
    datastore: Arc<DataStore>,
}

type ProxyContext = Context<ProxyState>;

#[serde_as]
#[derive(Debug, serde::Serialize)]
struct RequestLogFields {
    #[serde_as(as = "DisplayFromStr")]
    method: Method,
    client_ip: String,
    http_host: String,
    #[serde_as(as = "DisplayFromStr")]
    url: Uri,
    response_bytes: usize,
    http_version: String,
}

pub async fn run_server(cli: Cli) -> Result<(), BurleyError> {
    let tls_config = match (&cli.tls_cert, &cli.tls_key) {
        (Some(cert), Some(key)) => Some(load_tls_acceptor_data(cert, key).await?),
        (None, None) => None,
        _ => {
            return Err(BurleyError::Other(
                "TLS cert and key must be provided together".to_owned(),
            ));
        }
    };

    let store_dir = tempdir()?;
    let state = ProxyState {
        datastore: Arc::new(DataStore::new(
            CACHE_MAX_SIZE as u64,
            store_dir.path().to_path_buf(),
        )),
    };

    let http_addr = format!("127.0.0.1:{}", cli.http_port);
    let http_listener = TcpListener::build_with_state(state.clone())
        .bind(http_addr.clone())
        .await?;

    let https_listener = if tls_config.is_some() {
        let https_addr = format!("127.0.0.1:{}", cli.https_port);
        Some((
            TcpListener::build_with_state(state)
                .bind(https_addr.clone())
                .await?,
            https_addr,
        ))
    } else {
        None
    };

    // let graceful = rama::graceful::Shutdown::default();

    let exec = Executor::new();

    // graceful.spawn_task_fn(async move |guard| {
    //     let exec = Executor::graceful(guard.clone());
    let http_service = HttpServer::auto(exec).service(
        (
            TraceLayer::new_for_http(),
            UpgradeLayer::new(
                MethodMatcher::CONNECT,
                service_fn(http_connect_accept),
                service_fn(http_connect_proxy),
            ),
            RemoveResponseHeaderLayer::hop_by_hop(),
            RemoveRequestHeaderLayer::hop_by_hop(),
        )
            .into_layer(service_fn(http_plain_proxy)),
    );
    info!(addr = %http_addr, "starting HTTP proxy listener");
    http_listener
        .serve(BodyLimitLayer::symmetric(BODY_LIMIT).into_layer(http_service))
        .await;
    // });

    if let (Some((https_listener, https_addr)), Some(tls_config)) = (https_listener, tls_config) {
        //     graceful.spawn_task_fn(async move |guard| {
        let exec = Executor::new();
        //         let exec = Executor::graceful(guard.clone());
        let http_service = HttpServer::auto(exec).service(
            (
                TraceLayer::new_for_http(),
                UpgradeLayer::new(
                    MethodMatcher::CONNECT,
                    service_fn(http_connect_accept),
                    service_fn(http_connect_proxy),
                ),
                RemoveResponseHeaderLayer::hop_by_hop(),
                RemoveRequestHeaderLayer::hop_by_hop(),
            )
                .into_layer(service_fn(http_plain_proxy)),
        );
        info!(addr = %https_addr, "starting HTTPS proxy listener");
        https_listener
            .serve(
                // guard,
                (
                    BodyLimitLayer::symmetric(BODY_LIMIT),
                    TlsAcceptorLayer::new(tls_config).with_store_client_hello(true),
                )
                    .into_layer(http_service),
            )
            .await;
        // });
    }

    // graceful
    //     .shutdown_with_limit(Duration::from_secs(30))
    //     .await
    //     .map_err(|err| BurleyError::Other(err.to_string()))?;

    Ok(())
}

async fn http_connect_accept(
    mut ctx: ProxyContext,
    req: Request,
) -> Result<(Response, ProxyContext, Request), Response> {
    match ctx.get_or_try_insert_with_ctx::<RequestContext, _>(|ctx| (ctx, &req).try_into()) {
        Ok(request_ctx) => debug!("accept CONNECT to {}", request_ctx.authority),
        Err(err) => {
            error!(err = %err, "error extracting CONNECT authority");
            return Err(StatusCode::BAD_REQUEST.into_response());
        }
    }

    log_request_fields(request_log_fields(&ctx, &req, 0));

    Ok((StatusCode::OK.into_response(), ctx, req))
}

async fn http_connect_proxy(ctx: ProxyContext, mut upgraded: Upgraded) -> Result<(), Infallible> {
    let Some(request_ctx) = ctx.get::<RequestContext>() else {
        error!("CONNECT request context missing");
        return Ok(());
    };

    let authority = request_ctx.authority.clone();
    debug!("CONNECT to {authority}");
    let (mut stream, _) = match default_tcp_connect(&ctx, authority).await {
        Ok(stream) => stream,
        Err(err) => {
            error!(error = %err, "error connecting to CONNECT host");
            return Ok(());
        }
    };

    if let Err(err) = tokio::io::copy_bidirectional(&mut upgraded, &mut stream).await
        && !is_connection_error(&err)
    {
        error!(error = %err, "error copying CONNECT tunnel data");
    }

    Ok(())
}

async fn http_plain_proxy(ctx: ProxyContext, req: Request) -> Result<Response, Infallible> {
    let method = req.method().clone();
    let cache_key = req.uri().to_string();
    let request_log = request_log_fields(&ctx, &req, 0);

    if method == Method::GET
        && let Some(entry) = ctx.state().datastore.get(&cache_key)
    {
        let response_bytes = entry.content.len();
        debug!(uri = %cache_key, "cache hit");
        log_request_fields(RequestLogFields {
            response_bytes,
            ..request_log
        });
        return Ok(response_from_cache(entry));
    }

    let client = EasyHttpWebClient::default();
    match client.serve(ctx.clone(), req).await {
        Ok(resp) => cache_and_tag_response(ctx, method, cache_key, request_log, resp).await,
        Err(err) => {
            error!(error = %err, "error in upstream request");
            log_request_fields(request_log);
            Ok(empty_response(StatusCode::INTERNAL_SERVER_ERROR))
        }
    }
}

async fn cache_and_tag_response(
    ctx: ProxyContext,
    method: Method,
    cache_key: String,
    mut request_log: RequestLogFields,
    resp: Response,
) -> Result<Response, Infallible> {
    let (mut parts, body) = resp.into_parts();
    let body_bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => {
            error!(error = %err, "error collecting upstream response body");
            return Ok(empty_response(StatusCode::INTERNAL_SERVER_ERROR));
        }
    };
    request_log.response_bytes = body_bytes.len();

    let max_body_len = ctx.state().datastore.max_store_size as usize;
    let should_cache = is_cacheable_response(
        &method,
        parts.status,
        &parts.headers,
        body_bytes.len(),
        max_body_len,
    );

    if should_cache {
        ctx.state().datastore.insert(
            cache_key.clone(),
            CacheEntry {
                content: body_bytes.to_vec(),
                headers: cache_headers(&parts.headers),
                status: parts.status,
                timestamp: Utc::now(),
            },
        );
    }

    set_cache_header(&mut parts.headers, "miss");
    log_request_fields(request_log);

    Ok(Response::from_parts(parts, Body::from(body_bytes)))
}

fn request_log_fields(
    ctx: &ProxyContext,
    req: &Request,
    response_bytes: usize,
) -> RequestLogFields {
    RequestLogFields {
        method: req.method().clone(),
        client_ip: ctx
            .get::<SocketInfo>()
            .map(|socket| socket.peer_addr().ip().to_string())
            .unwrap_or_else(|| "-".to_owned()),
        http_host: req.uri().host().unwrap_or("-").to_owned(),
        url: req.uri().clone(),
        response_bytes,
        http_version: format!("{:?}", req.version()),
    }
}

fn log_request_fields(fields: RequestLogFields) {
    info!("{}", serde_json::json!(fields));
}

fn response_from_cache(entry: CacheEntry) -> Response {
    let mut response = Response::new(Body::from(entry.content));
    *response.status_mut() = entry.status;
    *response.headers_mut() = entry.headers;
    set_cache_header(response.headers_mut(), "hit");
    response
}

fn cache_headers(headers: &HeaderMap) -> HeaderMap {
    let mut cached = headers.clone();
    cached.remove(header::TRANSFER_ENCODING);
    cached.remove(header::CONNECTION);
    cached
}

fn set_cache_header(headers: &mut HeaderMap, value: &'static str) {
    headers.insert(
        HeaderName::from_static(CACHE_HEADER),
        HeaderValue::from_static(value),
    );
}

fn empty_response(status: StatusCode) -> Response {
    match Response::builder().status(status).body(Body::empty()) {
        Ok(response) => response,
        Err(err) => {
            error!(error = %err, "error building empty response");
            Response::new(Body::empty())
        }
    }
}

async fn load_tls_acceptor_data(
    cert_path: &Path,
    key_path: &Path,
) -> Result<TlsAcceptorData, BurleyError> {
    let cert_chain = tokio::fs::read(cert_path).await?;
    let private_key = tokio::fs::read(key_path).await?;
    let (cert_chain, key_der) = parse_certificate(&cert_chain, &private_key)?;

    let data = TlsAcceptorDataBuilder::new(cert_chain, key_der)
        .context("build TLS acceptor data")?
        .with_alpn_protocols_http_auto()
        .with_env_key_logger()
        .context("configure TLS key logger")?
        .build();

    Ok(data)
}

fn parse_certificate(
    cert_chain: &[u8],
    private_key: &[u8],
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), OpaqueError> {
    let cert_chain = pemfile::certs(&mut BufReader::new(cert_chain))
        .collect::<Result<Vec<_>, _>>()
        .context("collect cert chain")?;

    let private_key = pemfile::private_key(&mut BufReader::new(private_key))
        .context("load private key")?
        .context("non-empty private key")?;

    Ok((cert_chain, private_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rama::http::dep::http_body_util::BodyExt;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_state() -> ProxyState {
        let store_dir = tempdir().expect("create temp store");
        ProxyState {
            datastore: Arc::new(DataStore::new(1024 * 1024, store_dir.path().to_path_buf())),
        }
    }

    async fn start_counting_http_upstream() -> (SocketAddr, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind upstream");
        let addr = listener.local_addr().expect("upstream local addr");
        let count = Arc::new(AtomicUsize::new(0));
        let count_for_task = count.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let count_for_connection = count_for_task.clone();
                tokio::spawn(async move {
                    let mut buf = [0_u8; 2048];
                    let _ = stream.read(&mut buf).await.expect("read request");
                    let request_count = count_for_connection.fetch_add(1, Ordering::SeqCst) + 1;
                    let body = format!("response-{request_count}");
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: text/plain\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream
                        .write_all(response.as_bytes())
                        .await
                        .expect("write response");
                });
            }
        });

        (addr, count)
    }

    #[tokio::test]
    async fn caches_repeated_plain_http_gets() {
        let (upstream_addr, request_count) = start_counting_http_upstream().await;
        let state = test_state();
        let ctx = Context::new(state, Executor::new());
        let uri = format!("http://{upstream_addr}/cached");

        let first_req = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .body(Body::empty())
            .expect("build first request");
        let first = http_plain_proxy(ctx.clone(), first_req)
            .await
            .expect("proxy first request");
        assert_eq!(first.headers()[CACHE_HEADER], "miss");
        let first_body = first.into_body().collect().await.expect("first body");
        assert_eq!(first_body.to_bytes().as_ref(), b"response-1");

        let second_req = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .body(Body::empty())
            .expect("build second request");
        let second = http_plain_proxy(ctx, second_req)
            .await
            .expect("proxy second request");
        assert_eq!(second.headers()[CACHE_HEADER], "hit");
        let second_body = second.into_body().collect().await.expect("second body");
        assert_eq!(second_body.to_bytes().as_ref(), b"response-1");
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn builds_request_log_fields_from_context_request_and_body_length() {
        let state = test_state();
        let mut ctx = Context::new(state, Executor::new());
        ctx.insert(rama::net::stream::SocketInfo::new(
            None,
            "203.0.113.7:54321".parse().expect("valid peer addr"),
        ));
        let req = Request::builder()
            .method(Method::POST)
            .version(rama::http::Version::HTTP_2)
            .uri("https://example.test/upload?part=1")
            .body(Body::empty())
            .expect("build request");

        let log = request_log_fields(&ctx, &req, 512);

        assert_eq!(log.method, Method::POST);
        assert_eq!(log.client_ip, "203.0.113.7");
        assert_eq!(log.url, "https://example.test/upload?part=1");
        assert_eq!(log.response_bytes, 512);
        assert_eq!(
            log.http_version,
            format!("{:?}", rama::http::Version::HTTP_2)
        );
    }

    async fn start_echo_tcp_upstream() -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind echo upstream");
        let addr = listener.local_addr().expect("echo upstream local addr");

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept echo client");
            let mut buf = [0_u8; 64];
            let read = stream.read(&mut buf).await.expect("read echo payload");
            stream
                .write_all(&buf[..read])
                .await
                .expect("write echo payload");
        });

        addr
    }

    async fn start_proxy_listener() -> (SocketAddr, tokio::task::JoinHandle<()>, Arc<DataStore>) {
        let store_dir = tempdir().expect("create temp store");
        let datastore = Arc::new(DataStore::new(1024 * 1024, store_dir.path().to_path_buf()));
        let state = ProxyState {
            datastore: datastore.clone(),
        };
        let listener = TcpListener::build_with_state(state)
            .bind("127.0.0.1:0")
            .await
            .expect("bind proxy");
        let addr = listener.local_addr().expect("proxy local addr");
        let service = HttpServer::auto(Executor::new()).service(
            (
                TraceLayer::new_for_http(),
                UpgradeLayer::new(
                    MethodMatcher::CONNECT,
                    service_fn(http_connect_accept),
                    service_fn(http_connect_proxy),
                ),
                RemoveResponseHeaderLayer::hop_by_hop(),
                RemoveRequestHeaderLayer::hop_by_hop(),
            )
                .into_layer(service_fn(http_plain_proxy)),
        );
        let handle = tokio::spawn(async move {
            listener
                .serve(BodyLimitLayer::symmetric(BODY_LIMIT).into_layer(service))
                .await;
        });

        (addr, handle, datastore)
    }

    #[tokio::test]
    async fn connect_tunnels_bytes_without_creating_cache_entries() {
        let upstream_addr = start_echo_tcp_upstream().await;
        let (proxy_addr, proxy_task, datastore) = start_proxy_listener().await;

        let mut proxy = tokio::net::TcpStream::connect(proxy_addr)
            .await
            .expect("connect to proxy");
        let connect = format!("CONNECT {upstream_addr} HTTP/1.1\r\nHost: {upstream_addr}\r\n\r\n");
        proxy
            .write_all(connect.as_bytes())
            .await
            .expect("write CONNECT request");

        let mut response = vec![0_u8; 1024];
        let read = proxy
            .read(&mut response)
            .await
            .expect("read CONNECT response");
        let response = String::from_utf8_lossy(&response[..read]);
        assert!(response.starts_with("HTTP/1.1 200"));

        proxy
            .write_all(b"hello through tunnel")
            .await
            .expect("write tunneled payload");
        let mut echoed = [0_u8; 32];
        let read = proxy
            .read(&mut echoed)
            .await
            .expect("read tunneled payload");
        assert_eq!(&echoed[..read], b"hello through tunnel");
        assert!(datastore.urls.read().expect("read cache").is_empty());

        proxy_task.abort();
    }
}
