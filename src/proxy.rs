/*
 * Built on example code from:
 * https://github.com/omjadas/hudsucker/blob/main/examples/log.rs
 */

use once_cell::sync::Lazy;
use std::{path::PathBuf, process::Stdio, sync::Mutex};

use hudsucker::{
    async_trait::async_trait,
    certificate_authority::RcgenAuthority,
    hyper::{Body, Request, Response, StatusCode, Uri},
    *,
};
use rcgen::*;

use std::fs;
use std::net::SocketAddr;
use std::path::Path;

use rustls_pemfile as pemfile;

use openssl::hash::MessageDigest;
use openssl::x509::X509;
use std::error::Error;
use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
}

fn data_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".local/share"));
    }

    None
}

// Global var for getting server address.
static SERVER: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new("http://localhost:443".to_string()));
#[derive(Clone)]
struct ProxyHandler;

pub fn set_proxy_addr(addr: String) {
    if addr.contains(' ') {
        let addr2 = addr.replace(' ', "");
        *SERVER.lock().unwrap() = addr2;
    } else {
        *SERVER.lock().unwrap() = addr;
    }

    tracing::info!("Set server to {}", SERVER.lock().unwrap());
}

#[async_trait]
impl HttpHandler for ProxyHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        mut req: Request<Body>,
    ) -> RequestOrResponse {
        let uri = req.uri().to_string();

        let should_intercept = uri.contains("hoyoverse.com")
            || uri.contains("mihoyo.com")
            || uri.contains("yuanshen.com")
            || uri.ends_with(".yuanshen.com:12401")
            || uri.contains("starrails.com")
            || uri.contains("bhsr.com")
            || uri.contains("bh3.com")
            || uri.contains("honkaiimpact3.com")
            || uri.contains("zenlesszonezero.com")
            || uri.contains("stellasora.global")
            || uri.contains("yostarplat.com");

        if should_intercept {
            // Handle CONNECTs
            if req.method().as_str() == "CONNECT" {
                tracing::info!("[PROXY] Handling CONNECT for {}", uri);
                let builder = Response::builder()
                    .header("DecryptEndpoint", "Created")
                    .status(StatusCode::OK);
                let res = builder.body(()).unwrap();

                // Respond to CONNECT
                *res.body()
            } else {
                let uri_path_and_query = req
                    .uri()
                    .path_and_query()
                    .map(|pq| pq.as_str())
                    .unwrap_or("/");
                // Create new URI.
                let new_uri_str = format!("{}{}", SERVER.lock().unwrap(), uri_path_and_query);
                let new_uri = new_uri_str.parse::<Uri>().unwrap();

                tracing::info!("[PROXY] Redirecting {} to {}", uri, new_uri);
                // Set request URI to the new one.
                *req.uri_mut() = new_uri;
            }
        }

        req.into()
    }

    async fn handle_response(
        &mut self,
        _context: &HttpContext,
        response: Response<Body>,
    ) -> Response<Body> {
        response
    }

    async fn should_intercept(&mut self, _ctx: &HttpContext, _req: &Request<Body>) -> bool {
        true
    }
}

/*
 * Install a certificate into Wine's registry by creating a .reg file (UTF-16LE BOM)
 * and invoking `wine regedit <regfile>`.
 *
 * cert_path: path to PEM or DER certificate file
 * wine_prefix: Optional path to a wine prefix (e.g. ~/.wine). If Some, set WINEPREFIX env var.
 */
#[allow(dead_code)]
pub fn install_cert_into_wine(
    cert_path: &Path,
    wine_prefix: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    tracing::info!("Installing certificate: {}", cert_path.display());

    let cert_bytes = fs::read(cert_path)?;
    let x509 = match X509::from_pem(&cert_bytes) {
        Ok(cert) => cert,
        Err(_) => X509::from_der(&cert_bytes)?,
    };

    let sha1 = x509.digest(MessageDigest::sha1())?;
    let sha1_hex = hex::encode_upper(sha1);

    tracing::info!("Certificate SHA-1: {}", sha1_hex);

    let der = x509.to_der()?;

    let mut hex_pairs = der
        .iter()
        .map(|b| format!("{:02x},", b))
        .collect::<String>();
    if hex_pairs.ends_with(',') {
        hex_pairs.pop();
    }

    let mut wrapped = String::new();
    let mut i = 0usize;
    while i < hex_pairs.len() {
        let end = std::cmp::min(i + 78, hex_pairs.len());
        wrapped.push_str(&hex_pairs[i..end]);
        if end != hex_pairs.len() {
            wrapped.push_str("\\\n  ");
        }
        i = end;
    }

    let reg_ascii = format!(
        "Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\\Software\\Microsoft\\SystemCertificates\\My\\Certificates\\{}]

\"Blob\"=hex:{}


",
        sha1_hex, wrapped
    );

    let mut utf16le: Vec<u8> = Vec::new();
    utf16le.push(0xFF);
    utf16le.push(0xFE);
    for code_unit in reg_ascii.encode_utf16() {
        utf16le.extend(&code_unit.to_le_bytes());
    }

    let mut tempfile = NamedTempFile::new()?;
    tempfile.write_all(&utf16le)?;
    let reg_path = tempfile.path().to_path_buf();

    tracing::info!("Registry file created at: {}", reg_path.display());

    let mut cmd = Command::new("wine");
    if let Some(prefix) = wine_prefix {
        cmd.env("WINEPREFIX", prefix);
        tracing::info!("Using WINEPREFIX={}", prefix.display());
    }
    cmd.arg("regedit")
        .arg(&reg_path)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    tracing::info!("Importing certificate into Wine...");
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("wine regedit exited with status: {}", status).into());
    }

    tracing::info!("Certificate installed successfully.");
    tracing::info!(
        "Registry key: HKEY_CURRENT_USER\\Software\\Microsoft\\SystemCertificates\\My\\Certificates\\{}",
        sha1_hex
    );
    tracing::info!("Registry file imported from: {}", reg_path.display());

    Ok(())
}

/**
 * Starts an HTTP(S) proxy server.
 */
pub async fn create_proxy(proxy_port: u16) -> tokio::task::JoinHandle<()> {
    let data_dir = match data_dir() {
        Some(dir) => dir,
        None => {
            panic!("Could not determine data directory");
        }
    };

    let cert_dir = data_dir.join("anime-games-proxy").join("ca");
    let pk_path = cert_dir.join("private.key");
    let ca_path = cert_dir.join("cert.crt");

    // Get the certificate and private key.
    let mut private_key_bytes: &[u8] = &match fs::read(&pk_path) {
        // Try regenerating the CA stuff and read it again. If that doesn't work, quit.
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Encountered {}. Regenerating CA cert and retrying...", e);
            generate_ca_files(&data_dir.join("anime-games-proxy"));

            fs::read(&pk_path).expect("Could not read private key")
        }
    };

    let mut ca_cert_bytes: &[u8] = &match fs::read(&ca_path) {
        // Try regenerating the CA stuff and read it again. If that doesn't work, quit.
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Encountered {}. Regenerating CA cert and retrying...", e);
            generate_ca_files(&data_dir.join("anime-games-proxy"));

            fs::read(&ca_path).expect("Could not read certificate")
        }
    };

    // Parse the private key and certificate.
    let private_key = rustls::PrivateKey(
        pemfile::pkcs8_private_keys(&mut private_key_bytes)
            .expect("Failed to parse private key")
            .remove(0),
    );

    let ca_cert = rustls::Certificate(
        pemfile::certs(&mut ca_cert_bytes)
            .expect("Failed to parse CA certificate")
            .remove(0),
    );

    // Create the certificate authority.
    let authority = RcgenAuthority::new(private_key, ca_cert, 1_000)
        .expect("Failed to create Certificate Authority");

    // Create an instance of the proxy.
    let proxy = ProxyBuilder::new()
        .with_addr(SocketAddr::from(([0, 0, 0, 0], proxy_port)))
        .with_rustls_client()
        .with_ca(authority)
        .with_http_handler(ProxyHandler)
        .build();

    tracing::info!("[PROXY] Starting proxy server on 0.0.0.0:{}", proxy_port);

    // Start the proxy.
    tokio::spawn(async move {
        if let Err(e) = proxy.start(shutdown_signal()).await {
            tracing::error!("[PROXY] Error running proxy: {}", e);
        }
        tracing::info!("[PROXY] Proxy server stopped");
    })
}

/*
 * Generates a private key and certificate used by the certificate authority.
 * Additionally installs the certificate and private key in the Root CA store.
 * Source: https://github.com/zu1k/good-mitm/raw/master/src/ca/gen.rs
 */
pub fn generate_ca_files(path: &Path) {
    let mut params = CertificateParams::default();
    let mut details = DistinguishedName::new();

    // Set certificate details.
    details.push(DnType::CommonName, "AnimeGamesProxy");
    details.push(DnType::OrganizationName, "AnimeGames");
    details.push(DnType::CountryName, "US");
    details.push(DnType::LocalityName, "Local");

    // Set details in the parameter.
    params.distinguished_name = details;
    // Set other properties.
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];

    // Create certificate.
    let cert = Certificate::from_params(params).unwrap();
    let cert_crt = cert.serialize_pem().unwrap();
    let private_key = cert.serialize_private_key_pem();

    // Make certificate directory.
    let cert_dir = path.join("ca");
    match fs::create_dir_all(&cert_dir) {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("Error creating certificate directory: {}", e);
        }
    };

    // Write the certificate to a file.
    let cert_path = cert_dir.join("cert.crt");
    match fs::write(&cert_path, cert_crt) {
        Ok(_) => tracing::info!("Wrote certificate to {}", cert_path.display()),
        Err(e) => tracing::error!(
            "Error writing certificate to {}: {}",
            cert_path.display(),
            e
        ),
    }

    // Write the private key to a file.
    let private_key_path = cert_dir.join("private.key");
    match fs::write(&private_key_path, private_key) {
        Ok(_) => tracing::info!("Wrote private key to {}", private_key_path.display()),
        Err(e) => tracing::error!(
            "Error writing private key to {}: {}",
            private_key_path.display(),
            e
        ),
    }

    // install_cert_into_wine(&cert_path, None).unwrap_or_else(|e| {
    //     tracing::error!("Failed to install certificate into Wine: {}", e);
    // });
}
