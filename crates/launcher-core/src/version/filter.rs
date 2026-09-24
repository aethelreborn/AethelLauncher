//! Filter versions by type, date range, and ID patterns.

use super::VersionInfo;

#[derive(Debug, Clone, Default)]
pub struct VersionFilter {
    /// If set, only include versions of this type.
    pub type_filter: Option<crate::version::VersionType>,
    /// If set, only include versions released after this unix timestamp.
    pub since: Option<u64>,
    /// If set, only include versions whose ID contains this substring.
    pub search: Option<String>,
}

impl VersionFilter {
    /// Apply the filter and return matching versions (preserving original order).
    pub fn apply(&self, versions: &[VersionInfo]) -> Vec<VersionInfo> {
        versions
            .iter()
            .filter(|v| self.matches(v))
            .cloned()
            .collect()
    }

    /// Group filtered versions by release year.
    pub fn group_by_year(&self, versions: &[VersionInfo]) -> Vec<(String, Vec<VersionInfo>)> {
        let mut grouped: std::collections::BTreeMap<u32, Vec<VersionInfo>> =
            std::collections::BTreeMap::new();
        for v in self.apply(versions) {
            grouped.entry(v.release_year()).or_default().push(v);
        }
        grouped
            .into_iter()
            .map(|(y, v)| (y.to_string(), v))
            .collect()
    }

    fn matches(&self, v: &VersionInfo) -> bool {
        if let Some(ref t) = self.type_filter {
            if v.type_ != *t {
                return false;
            }
        }
        if let Some(since) = self.since {
            let ts = v.release_time.unwrap_or(v.time).timestamp() as u64;
            if ts < since {
                return false;
            }
        }
        if let Some(ref search) = self.search {
            if !v.id.to_lowercase().contains(&search.to_lowercase()) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::VersionType;
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
    fn filter_release_only() {
        let versions = vec![
            make("1.21.1", VersionType::Release, 1700000000),
            make("25w14a", VersionType::Snapshot, 1700000000),
            make("1.21.0", VersionType::Release, 1690000000),
        ];
        let filter = VersionFilter {
            type_filter: Some(VersionType::Release),
            ..Default::default()
        };
        let result = filter.apply(&versions);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|v| v.is_release()));
    }

    #[test]
    fn filter_since_timestamp() {
        let versions = vec![
            make("1.21.1", VersionType::Release, 1700000000),
            make("1.20.4", VersionType::Release, 1690000000),
        ];
        let filter = VersionFilter {
            since: Some(1695000000),
            ..Default::default()
        };
        let result = filter.apply(&versions);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "1.21.1");
    }

    #[test]
    fn filter_search_substring() {
        let versions = vec![
            make("1.21.1", VersionType::Release, 1700000000),
            make("1.21.0", VersionType::Release, 1695000000),
            make("1.20.4", VersionType::Release, 1690000000),
        ];
        let filter = VersionFilter {
            search: Some("1.21".to_string()),
            ..Default::default()
        };
        let result = filter.apply(&versions);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn group_by_year() {
        let versions = vec![
            make("1.21.1", VersionType::Release, 1700000000),
            make("1.20.4", VersionType::Release, 1690000000),
            make("25w14a", VersionType::Snapshot, 1720000000),
        ];
        let filter = VersionFilter::default();
        let grouped = filter.group_by_year(&versions);
        assert_eq!(grouped.len(), 2);
        let y2023 = grouped
            .iter()
            .find(|(y, _)| *y == "2023")
            .map(|(_, v)| v.len())
            .unwrap_or(0);
        assert_eq!(y2023, 2);
        let y2024 = grouped
            .iter()
            .find(|(y, _)| *y == "2024")
            .map(|(_, v)| v.len())
            .unwrap_or(0);
        assert_eq!(y2024, 1);
    }
}
