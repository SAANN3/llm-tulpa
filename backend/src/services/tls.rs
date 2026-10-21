//! The HTTPS certificate of the frontend container (see `frontend/`'s `docker-entrypoint.d`): the one it made itself
//! and the one the owner uploaded in Settings. Both live in `tls_dir`, which the container mounts: `generated/` is the
//! container's, `custom/` is written here, and the container serves `custom/` while it is there. This module checks
//! what is uploaded (a certificate and the key that belongs to it, not expired) and says which is in use.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::StatusCode;
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use serde::Serialize;
use utoipa::ToSchema;
use x509_parser::prelude::*;

use super::error::ErrorService;

/// A certificate and key are a few KB; anything much bigger is not one
const MAX_PEM_BYTES: usize = 64 * 1024;
const CERT_FILE: &str = "cert.pem";
const KEY_FILE: &str = "key.pem";
/// The names the generated certificate is to cover besides those the container was started with, one per line
const EXTRA_NAMES_FILE: &str = "extra-names.txt";
/// Its presence asks the container for a new generated certificate; the container takes it away once it has made one
const REGENERATE_FILE: &str = "regenerate";

/// What to show of a certificate
#[derive(Serialize, ToSchema, Clone, Debug, PartialEq)]
pub struct CertInfo {
    /// Whose it is, as its subject names it
    pub subject: String,
    /// The names and addresses it is valid for
    pub names: Vec<String>,
    /// RFC 3339
    pub not_after: String,
    pub expired: bool,
}

#[derive(Serialize, ToSchema)]
pub struct TlsStatus {
    /// This machine's addresses on the networks a phone or another computer would reach it by (IPv4, without loopback,
    /// Docker's bridges and the like): what to type after `https://` on another device
    pub addresses: Vec<String>,
    /// A new certificate has been asked for and the container has not made it yet
    pub regenerating: bool,
    /// `custom` (uploaded), `generated` (made by the container) or `none` (the container has not made one yet, or
    /// there is no https)
    pub active: String,
    pub custom: Option<CertInfo>,
    pub generated: Option<CertInfo>,
}

pub struct TlsStore {
    dir: PathBuf,
}

fn bad(message: impl Into<String>) -> ErrorService {
    ErrorService::new(StatusCode::BAD_REQUEST, message)
}

fn describe(der: &CertificateDer<'_>) -> Result<CertInfo, String> {
    let (_, cert) = X509Certificate::from_der(der).map_err(|e| format!("not a readable certificate: {e}"))?;
    let mut names = Vec::new();
    if let Ok(Some(san)) = cert.subject_alternative_name() {
        for name in &san.value.general_names {
            match name {
                GeneralName::DNSName(dns) => names.push(dns.to_string()),
                GeneralName::IPAddress(bytes) => match bytes.len() {
                    4 => names.push(std::net::Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]).to_string()),
                    16 => {
                        let mut octets = [0u8; 16];
                        octets.copy_from_slice(bytes);
                        names.push(std::net::Ipv6Addr::from(octets).to_string());
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    let not_after = cert.validity().not_after;
    let expired = !cert.validity().is_valid();
    let when = chrono::DateTime::from_timestamp(not_after.timestamp(), 0).map(|t| t.to_rfc3339()).unwrap_or_default();
    Ok(CertInfo { subject: cert.subject().to_string(), names, not_after: when, expired })
}

/// The certificates of a PEM text, leaf first
fn certificates(pem: &str) -> Result<Vec<CertificateDer<'static>>, ErrorService> {
    let chain: Vec<_> = CertificateDer::pem_slice_iter(pem.as_bytes()).collect::<Result<_, _>>().map_err(|e| bad(format!("the certificate is not valid PEM: {e}")))?;
    if chain.is_empty() {
        return Err(bad("the certificate holds no `-----BEGIN CERTIFICATE-----` block"));
    }
    Ok(chain)
}

/// Checks that `certificate` and `private_key` are PEM, belong together and are still valid; what it says of the
/// certificate when they do
pub fn check(certificate: &str, private_key: &str) -> Result<CertInfo, ErrorService> {
    if certificate.len() > MAX_PEM_BYTES || private_key.len() > MAX_PEM_BYTES {
        return Err(bad("that is too big to be a certificate or a key"));
    }
    let chain = certificates(certificate)?;
    let key = PrivateKeyDer::from_pem_slice(private_key.as_bytes()).map_err(|_| {
        bad("the private key is not a PEM key (`-----BEGIN PRIVATE KEY-----`, `RSA PRIVATE KEY` or `EC PRIVATE KEY`); a key with a password is not supported")
    })?;
    let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key).map_err(|e| bad(format!("the private key is not one that can be used: {e}")))?;
    rustls::sign::CertifiedKey::new(chain.clone(), signing).keys_match().map_err(|_| bad("the private key does not belong to the certificate"))?;
    let info = describe(&chain[0]).map_err(bad)?;
    if info.expired {
        return Err(bad(format!("the certificate has expired ({})", info.not_after)));
    }
    Ok(info)
}

/// Whether `name` is something a certificate can be made for and that is safe to hand to a shell script: a host name or
/// an IP address, nothing else
fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 253
        && (name.parse::<std::net::IpAddr>().is_ok()
            || (name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') && !name.starts_with(['-', '.'])))
}

/// Whether a network interface is one a phone or another computer reaches this machine by: not the loopback, not what
/// Docker, a bridge, a VM or a tunnel makes up on the machine itself
fn is_lan_interface(name: &str) -> bool {
    const MADE_UP: [&str; 8] = ["lo", "docker", "br-", "veth", "virbr", "cni", "flannel", "cali"];
    let name = name.to_lowercase();
    !MADE_UP.iter().any(|prefix| name.starts_with(prefix)) && !name.contains("tun") && !name.starts_with("tap")
}

/// This machine's IPv4 addresses worth telling another device (see `is_lan_interface`)
pub fn lan_addresses() -> Vec<String> {
    let mut found = Vec::new();
    for (name, network) in sysinfo::Networks::new_with_refreshed_list().iter() {
        if !is_lan_interface(name) {
            continue;
        }
        for ip in network.ip_networks() {
            if let std::net::IpAddr::V4(v4) = ip.addr {
                if !v4.is_loopback() && !v4.is_link_local() && !found.contains(&v4.to_string()) {
                    found.push(v4.to_string());
                }
            }
        }
    }
    found
}

/// The names a certificate for this machine should cover now: `localhost`, its host name, every address of its network
/// interfaces and, when it is a name or an address, `used` (the one the owner reached the page by, which is the one a
/// browser has to find on it)
pub fn names_of_this_machine(used: Option<&str>) -> Vec<String> {
    let mut names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    if let Some(host) = sysinfo::System::host_name() {
        names.push(host);
    }
    for (_, network) in sysinfo::Networks::new_with_refreshed_list().iter() {
        for ip in network.ip_networks() {
            let ip = ip.addr;
            let link_local = match ip {
                std::net::IpAddr::V4(v4) => v4.is_link_local(),
                std::net::IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
            };
            if !ip.is_loopback() && !link_local {
                names.push(ip.to_string());
            }
        }
    }
    names.extend(used.map(str::to_string));
    let mut kept = Vec::new();
    for name in names {
        if is_name(&name) && !kept.contains(&name) {
            kept.push(name);
        }
    }
    kept
}

impl TlsStore {
    pub fn new(dir: PathBuf) -> Arc<Self> {
        Arc::new(Self { dir })
    }

    fn info_of(&self, which: &str) -> Option<CertInfo> {
        let pem = std::fs::read_to_string(self.dir.join(which).join(CERT_FILE)).ok()?;
        describe(certificates(&pem).ok()?.first()?).ok()
    }

    pub fn status(&self) -> TlsStatus {
        let custom = self.info_of("custom");
        let generated = self.info_of("generated");
        let active = if custom.is_some() { "custom" } else if generated.is_some() { "generated" } else { "none" };
        TlsStatus { addresses: lan_addresses(), regenerating: self.dir.join(REGENERATE_FILE).exists(), active: active.to_string(), custom, generated }
    }

    /// Stores an uploaded certificate and key for the container to serve. Both are written beside their place first
    /// and moved into it, the key before the certificate, so the container never finds half of a pair.
    pub fn set_custom(&self, certificate: &str, private_key: &str) -> Result<CertInfo, ErrorService> {
        let info = check(certificate, private_key)?;
        let custom = self.dir.join("custom");
        let io = |e: std::io::Error| {
            tracing::warn!("could not store the certificate in {}: {e}", custom.display());
            ErrorService::internal(format!("could not write the certificate into {}", custom.display()))
        };
        std::fs::create_dir_all(&custom).map_err(io)?;
        for (file, text, mode) in [(KEY_FILE, private_key, 0o600), (CERT_FILE, certificate, 0o644)] {
            let temp = custom.join(format!("{file}.new"));
            write_private(&temp, text, mode).map_err(io)?;
            std::fs::rename(&temp, custom.join(file)).map_err(io)?;
        }
        Ok(info)
    }

    /// Asks the container for a new self-signed certificate that covers `names` as well as the ones it was started with.
    /// The container notices within seconds; the names are written first, the request after.
    pub fn regenerate(&self, names: &[String]) -> Result<(), ErrorService> {
        let io = |e: std::io::Error| {
            tracing::warn!("could not ask for a new certificate in {}: {e}", self.dir.display());
            ErrorService::internal(format!("could not write into {}", self.dir.display()))
        };
        std::fs::create_dir_all(&self.dir).map_err(io)?;
        let temp = self.dir.join(format!("{EXTRA_NAMES_FILE}.new"));
        write_private(&temp, &format!("{}\n", names.join("\n")), 0o644).map_err(io)?;
        std::fs::rename(&temp, self.dir.join(EXTRA_NAMES_FILE)).map_err(io)?;
        write_private(&self.dir.join(REGENERATE_FILE), "", 0o644).map_err(io)
    }

    /// Takes the uploaded certificate away, so the container serves the one it made
    pub fn remove_custom(&self) -> Result<(), ErrorService> {
        let custom = self.dir.join("custom");
        // The certificate goes first: the container takes a missing one to mean "no custom certificate"
        for file in [CERT_FILE, KEY_FILE] {
            match std::fs::remove_file(custom.join(file)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!("could not remove {}: {e}", custom.join(file).display());
                    return Err(ErrorService::internal("could not remove the certificate"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn write_private(path: &Path, text: &str, mode: u32) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(mode).open(path)?;
    file.write_all(text.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, text: &str, _mode: u32) -> std::io::Result<()> {
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A self-signed certificate and key made with the openssl command line, for the tests
    fn openssl_pair(days: i32) -> Option<(String, String)> {
        let dir = std::env::temp_dir().join(format!("tls-test-{}-{days}", std::process::id()));
        std::fs::create_dir_all(&dir).ok()?;
        let ok = std::process::Command::new("openssl")
            .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "key.pem", "-out", "cert.pem", "-days", &days.to_string(), "-subj", "/CN=test.local", "-addext", "subjectAltName=DNS:test.local,IP:192.168.1.5"])
            .current_dir(&dir)
            .output()
            .ok()?
            .status
            .success();
        let pair = ok.then(|| (std::fs::read_to_string(dir.join("cert.pem")).unwrap(), std::fs::read_to_string(dir.join("key.pem")).unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
        pair
    }

    #[test]
    fn a_matching_pair_is_accepted_and_described_and_a_mismatched_or_broken_one_is_not() {
        let Some((cert, key)) = openssl_pair(30) else {
            eprintln!("openssl is not installed: skipped");
            return;
        };
        let info = check(&cert, &key).ok().unwrap();
        assert!(info.subject.contains("test.local"), "{info:?}");
        assert!(info.names.contains(&"192.168.1.5".to_string()) && info.names.contains(&"test.local".to_string()), "{info:?}");
        assert!(!info.expired);

        let (_, other_key) = openssl_pair(31).unwrap();
        assert!(check(&cert, &other_key).err().unwrap().message.unwrap().contains("does not belong"));
        assert!(check("not a certificate", &key).is_err());
        assert!(check(&cert, "not a key").is_err());
        assert!(check(&key, &key).is_err(), "a key where the certificate goes");
    }

    #[test]
    fn names_for_a_certificate_are_this_machines_and_only_names() {
        let names = names_of_this_machine(Some("192.168.77.5"));
        assert!(names.contains(&"localhost".to_string()) && names.contains(&"127.0.0.1".to_string()));
        assert_eq!(names.last().map(String::as_str), Some("192.168.77.5"), "the address in use is on it: {names:?}");
        assert!(names.iter().all(|n| is_name(n)), "{names:?}");
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "each once");
        // Anything that is not a name or an address is left out, however it got there
        for bad in ["", "a b", "x;rm -rf /", "$(id)", "-flag", ".hidden", "a/b", &"a".repeat(300)] {
            assert!(!is_name(bad), "{bad:?}");
        }
        for good in ["my-pc.local", "192.168.1.5", "::1", "fd00::5", "Host.Example.COM"] {
            assert!(is_name(good), "{good}");
        }
        assert!(!names_of_this_machine(Some("x;y")).iter().any(|n| n.contains(';')));
    }

    #[test]
    fn only_interfaces_another_device_could_reach_are_offered() {
        for lan in ["enp37s0", "eth0", "wlan0", "wlp3s0", "Ethernet", "tailscale0", "en0"] {
            assert!(is_lan_interface(lan), "{lan}");
        }
        for made_up in ["lo", "docker0", "br-18b08377e109", "veth12ab", "virbr0", "singbox_tun", "tun0", "tap0", "cni0", "flannel.1", "cali123"] {
            assert!(!is_lan_interface(made_up), "{made_up}");
        }
        assert!(lan_addresses().iter().all(|a| a.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| !ip.is_loopback())));
    }

    #[test]
    fn a_new_certificate_is_asked_for_by_writing_the_names_and_then_the_request() {
        let dir = std::env::temp_dir().join(format!("tls-regenerate-{}", std::process::id()));
        let store = TlsStore::new(dir.clone());
        assert!(!store.status().regenerating);
        store.regenerate(&["a.local".to_string(), "10.1.2.3".to_string()]).ok().unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("extra-names.txt")).unwrap(), "a.local\n10.1.2.3\n");
        assert!(dir.join("regenerate").is_file() && store.status().regenerating);
        // The container takes the request away once it has made the certificate
        std::fs::remove_file(dir.join("regenerate")).unwrap();
        assert!(!store.status().regenerating);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn what_is_uploaded_is_stored_for_the_container_and_taken_away_again() {
        let Some((cert, key)) = openssl_pair(32) else { return };
        let dir = std::env::temp_dir().join(format!("tls-store-{}", std::process::id()));
        let store = TlsStore::new(dir.clone());
        assert_eq!(store.status().active, "none");
        store.set_custom(&cert, &key).ok().unwrap();
        let status = store.status();
        assert_eq!(status.active, "custom");
        assert!(status.custom.unwrap().names.contains(&"test.local".to_string()));
        assert!(dir.join("custom/key.pem").is_file() && !dir.join("custom/key.pem.new").exists());
        store.remove_custom().ok().unwrap();
        assert_eq!(store.status().active, "none");
        store.remove_custom().ok().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
