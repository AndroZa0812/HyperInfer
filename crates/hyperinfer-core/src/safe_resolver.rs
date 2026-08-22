use reqwest::dns::{Addrs, Name, Resolve};
use std::net::{IpAddr, ToSocketAddrs};

#[derive(Clone, Default)]
pub struct SafeResolver;

impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> reqwest::dns::Resolving {
        let name_str = name.as_str().to_string();
        Box::pin(async move {
            let addrs = tokio::task::spawn_blocking(move || {
                let addrs = (name_str.as_str(), 0).to_socket_addrs()?;
                let mut safe_addrs = Vec::new();
                for addr in addrs {
                    let ip = addr.ip();
                    let is_blocked = match ip {
                        IpAddr::V4(v4) => {
                            let o = v4.octets();
                            o[0] == 10
                                || (o[0] == 172 && o[1] >= 16 && o[1] <= 31)
                                || (o[0] == 192 && o[1] == 168)
                                || o[0] == 127
                                || (o[0] == 169 && o[1] == 254)
                                || o[0] == 0
                                || v4.is_broadcast()
                                || v4.is_unspecified()
                                || v4.is_documentation()
                                || v4.is_link_local()
                        }
                        IpAddr::V6(v6) => {
                            if let Some(v4) = v6.to_ipv4_mapped() {
                                let o = v4.octets();
                                o[0] == 10
                                    || (o[0] == 172 && o[1] >= 16 && o[1] <= 31)
                                    || (o[0] == 192 && o[1] == 168)
                                    || o[0] == 127
                                    || (o[0] == 169 && o[1] == 254)
                                    || o[0] == 0
                                    || v4.is_broadcast()
                                    || v4.is_unspecified()
                                    || v4.is_documentation()
                                    || v4.is_link_local()
                            } else {
                                let o = v6.octets();
                                v6.is_loopback()
                                    || v6.is_unspecified()
                                    || (o[0] & 0xfe == 0xfc)
                                    || (o[0] == 0xfe && (o[1] & 0xc0) == 0x80)
                            }
                        }
                    };
                    if !is_blocked {
                        safe_addrs.push(addr);
                    }
                }
                if safe_addrs.is_empty() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "All resolved IPs are blocked",
                    ));
                }
                Ok(safe_addrs)
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })??;

            let addrs_iter: Addrs = Box::new(addrs.into_iter());
            Ok(addrs_iter)
        })
    }
}
