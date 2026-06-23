use std::env;

const UNKNOWN_HOSTNAME: &str = "this device";

pub(crate) fn current_hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|value| normalize_hostname(&value.to_string_lossy()))
        .or_else(|| env_hostname("COMPUTERNAME"))
        .or_else(|| env_hostname("HOSTNAME"))
        .unwrap_or_else(|| UNKNOWN_HOSTNAME.to_string())
}

fn env_hostname(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .and_then(|value| normalize_hostname(&value))
}

fn normalize_hostname(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('.').trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_hostname;

    #[test]
    fn normalizes_hostnames_for_display() {
        assert_eq!(
            normalize_hostname(" demo-host. ").as_deref(),
            Some("demo-host")
        );
        assert_eq!(normalize_hostname(".").as_deref(), None);
        assert_eq!(normalize_hostname(" ").as_deref(), None);
    }
}
