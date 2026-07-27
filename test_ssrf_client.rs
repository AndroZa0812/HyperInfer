use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio;

struct SafeResolver;

impl reqwest::dns::Resolve for SafeResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let name_str = name.as_str().to_string();
            tokio::task::spawn_blocking(move || {
                let addrs = (name_str.as_str(), 0).to_socket_addrs()?;
                let mut safe_addrs = Vec::new();
                for addr in addrs {
                    let mut current_ip = addr.ip();
                    if let std::net::IpAddr::V6(v6) = current_ip {
                        if let Some(v4) = v6.to_ipv4_mapped() {
                            current_ip = std::net::IpAddr::V4(v4);
                        }
                    }

                    let is_blocked = match current_ip {
                        std::net::IpAddr::V4(v4) => {
                            v4.is_private()
                                || v4.is_loopback()
                                || v4.is_link_local()
                                || v4.is_broadcast()
                                || v4.is_unspecified()
                                || v4.is_documentation()
                        }
                        std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
                    };

                    if is_blocked {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Blocked IP",
                        ));
                    }
                    safe_addrs.push(addr);
                }
                Ok(Box::new(safe_addrs.into_iter()) as reqwest::dns::Addrs)
            })
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

#[tokio::main]
async fn main() {
    let client = reqwest::Client::builder()
        .dns_resolver(Arc::new(SafeResolver))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build HTTP client");

    let res = client.get("http://localhost").send().await;
    println!("localhost: {:?}", res.is_err());

    let res = client.get("http://127.0.0.1").send().await;
    println!("127.0.0.1: {:?}", res.is_err());

    let res = client.get("http://example.com").send().await;
    println!("example.com: {:?}", res.is_ok());
}
