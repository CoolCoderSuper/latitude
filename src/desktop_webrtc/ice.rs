use std::net::IpAddr;

use tracing::info;

pub(super) fn log_candidates(description: &str, sdp: &str) {
    let mut host = 0;
    let mut server_reflexive = 0;
    let mut relay = 0;
    let mut peer_reflexive = 0;
    let mut mdns = 0;

    for line in sdp.lines().filter(|line| line.starts_with("a=candidate:")) {
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        if fields
            .get(4)
            .is_some_and(|address| address.ends_with(".local"))
        {
            mdns += 1;
        }
        if let Some(candidate_type) = fields
            .windows(2)
            .find_map(|fields| (fields[0] == "typ").then_some(fields[1]))
        {
            match candidate_type {
                "host" => host += 1,
                "srflx" => server_reflexive += 1,
                "relay" => relay += 1,
                "prflx" => peer_reflexive += 1,
                _ => {}
            }
        }
    }

    info!(
        description,
        host, server_reflexive, relay, peer_reflexive, mdns, "WebRTC ICE candidates gathered"
    );
}

pub(super) fn rewrite_mdns_candidates(sdp: &str, peer_ip: Option<IpAddr>) -> (String, usize) {
    let Some(IpAddr::V4(peer_ip)) = peer_ip else {
        return (sdp.to_owned(), 0);
    };
    let peer_ip = peer_ip.to_string();
    let mut rewritten = 0;
    let lines = sdp
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split_ascii_whitespace().collect();
            let is_mdns_host = line.starts_with("a=candidate:")
                && fields
                    .get(4)
                    .is_some_and(|address| address.ends_with(".local"))
                && fields
                    .windows(2)
                    .any(|fields| fields[0] == "typ" && fields[1] == "host");
            if is_mdns_host {
                rewritten += 1;
                line.replacen(fields[4], &peer_ip, 1)
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\r\n");

    (format!("{lines}\r\n"), rewritten)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::rewrite_mdns_candidates;

    #[test]
    fn rewrites_only_mdns_host_candidate_addresses() {
        let sdp = concat!(
            "v=0\r\n",
            "a=candidate:host 1 udp 1 browser.local 5000 typ host\r\n",
            "a=candidate:srflx 1 udp 1 203.0.113.4 5001 typ srflx raddr 0.0.0.0 rport 0\r\n",
        );
        let (rewritten, count) =
            rewrite_mdns_candidates(sdp, Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));

        assert_eq!(count, 1);
        assert!(rewritten.contains("a=candidate:host 1 udp 1 127.0.0.1 5000 typ host"));
        assert!(rewritten.contains("a=candidate:srflx 1 udp 1 203.0.113.4 5001 typ srflx"));
        assert!(!rewritten.contains("browser.local"));
    }
}
