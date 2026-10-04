use super::{filter::VersionFilter, VersionInfo, VersionType};

#[derive(Debug, Clone, Default)]
pub struct VersionResolver {
    filter: VersionFilter,
}

impl VersionResolver {
    pub fn new(filter: VersionFilter) -> Self {
        Self { filter }
    }

    pub fn latest_release(&self, versions: &[VersionInfo]) -> Option<VersionInfo> {
        self.filter
            .apply(versions)
            .into_iter()
            .filter(|v| v.type_ == VersionType::Release)
            .max_by_key(|v| v.time)
    }

    pub fn latest_snapshot(&self, versions: &[VersionInfo]) -> Option<VersionInfo> {
        self.filter
            .apply(versions)
            .into_iter()
            .filter(|v| v.type_ == VersionType::Snapshot)
            .max_by_key(|v| v.time)
            .and_then(|v| {
                let first = v.id.chars().next()?;
                if first.is_ascii_digit() {
                    Some(v)
                } else {
                    None
                }
            })
    }

    pub fn group_by_minor(&self, versions: &[VersionInfo]) -> Vec<(String, Vec<VersionInfo>)> {
        let mut grouped: std::collections::BTreeMap<String, Vec<VersionInfo>> =
            std::collections::BTreeMap::new();
        for v in self.filter.apply(versions) {
            if let Some(minor) = extract_minor(&v.id) {
                grouped.entry(minor).or_default().push(v);
            }
        }
        grouped.into_iter().collect()
    }

    pub fn latest_tags(&self, versions: &[VersionInfo]) -> Vec<LatestTag> {
        let mut tags = Vec::new();
        if let Some(v) = self.latest_release(versions) {
            tags.push(LatestTag {
                label: "Latest Release".to_string(),
                version: v,
            });
        }
        if let Some(v) = self.latest_snapshot(versions) {
            tags.push(LatestTag {
                label: "Latest Snapshot".to_string(),
                version: v,
            });
        }
        tags
    }
}

#[derive(Debug, Clone)]
pub struct LatestTag {
    pub label: String,
    pub version: VersionInfo,
}

fn extract_minor(id: &str) -> Option<String> {
    let parts: Vec<&str> = id.split('.').collect();
    if parts.len() >= 2 {
        let major: u32 = parts[0].parse().ok()?;
        let minor_str: String = parts[1]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let _minor: u32 = minor_str.parse().ok()?;
        Some(format!("{major}.{minor_str}"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make(id: &str, type_: VersionType, ts: i64) -> VersionInfo {
        VersionInfo {
            id: id.to_string(),
            type_,
            url: format!("https://example.com/{id}.json"),
            time: Utc.timestamp_opt(ts, 0).unwrap(),
            release_time: Some(Utc.timestamp_opt(ts, 0).unwrap()),
        }
    }

    #[test]
    fn latest_release_returns_newest() {
        let versions = vec![
            make("1.20.4", VersionType::Release, 1690000000),
            make("1.21.1", VersionType::Release, 1700000000),
            make("1.21.0", VersionType::Release, 1695000000),
        ];
        let resolver = VersionResolver::default();
        assert_eq!(resolver.latest_release(&versions).unwrap().id, "1.21.1");
    }

    #[test]
    fn latest_snapshot_returns_newest() {
        let versions = vec![
            make("25w14a", VersionType::Snapshot, 1750000000),
            make("25w13a", VersionType::Snapshot, 1749000000),
        ];
        let resolver = VersionResolver::default();
        assert_eq!(resolver.latest_snapshot(&versions).unwrap().id, "25w14a");
    }

    #[test]
    fn group_by_minor_works() {
        let versions = vec![
            make("1.21.1", VersionType::Release, 1700000000),
            make("1.21.0", VersionType::Release, 1695000000),
            make("1.20.4", VersionType::Release, 1690000000),
            make("25w14a", VersionType::Snapshot, 1750000000),
        ];
        let resolver = VersionResolver::default();
        let groups = resolver.group_by_minor(&versions);
        let g1_21: Vec<_> = groups
            .iter()
            .filter(|(k, _)| *k == "1.21")
            .map(|(_, v)| v.len())
            .collect();
        assert_eq!(g1_21, vec![2]);
        let g1_20: Vec<_> = groups
            .iter()
            .filter(|(k, _)| *k == "1.20")
            .map(|(_, v)| v.len())
            .collect();
        assert_eq!(g1_20, vec![1]);
    }

    #[test]
    fn extract_minor_version() {
        assert_eq!(extract_minor("1.21.1"), Some("1.21".to_string()));
        assert_eq!(extract_minor("1.20.4"), Some("1.20".to_string()));
        assert_eq!(extract_minor("25w14a"), None);
    }
}
