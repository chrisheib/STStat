use itertools::Itertools;
use serde::Deserialize;
use std::str::FromStr;

#[derive(Deserialize, Default, Debug, Clone)]
#[allow(dead_code)]
#[allow(non_snake_case)]
pub struct OHWNode {
    pub Children: Vec<OHWNode>,
    pub ImageURL: String,
    pub Max: String,
    pub Min: String,
    pub Text: String,
    pub Value: String,
    pub id: i64,
}

pub trait MyNode {
    fn parse_value_path<T: FromStr + Default>(&self, path: &str) -> Option<T>;

    fn select(&self, path: &str) -> Option<&OHWNode>;

    fn parse_value_path_def<T: FromStr + Default>(&self, path: &str) -> T;
}

impl MyNode for Option<OHWNode> {
    fn parse_value_path<T: FromStr + Default>(&self, path: &str) -> Option<T> {
        if let Some(n) = self {
            n.parse_value_path(path)
        } else {
            None
        }
    }

    fn parse_value_path_def<T: FromStr + Default>(&self, path: &str) -> T {
        if let Some(n) = self {
            n.parse_value_path(path).unwrap_or_default()
        } else {
            Default::default()
        }
    }

    fn select(&self, path: &str) -> Option<&OHWNode> {
        if let Some(n) = self {
            n.select(path)
        } else {
            None
        }
    }
}

impl MyNode for OHWNode {
    fn parse_value_path<T: FromStr + Default>(&self, path: &str) -> Option<T> {
        if let Some(n) = self.select(path) {
            let v = n
                .Value
                .replace("°C", "")
                .replace('%', "")
                .replace("MHz", "")
                .replace('W', "")
                .replace("MB", "")
                .replace("GB", "")
                .replace(',', ".");
            let v = v.trim();

            Some(v.parse::<T>().unwrap_or_default())
        } else {
            None
        }
    }

    fn select(&self, path: &str) -> Option<&OHWNode> {
        let paths = path.split('|').collect_vec();
        let mut current = Some(self);
        for p in paths {
            if p.starts_with('#') {
                if let Ok(idx) = p.replace('#', "").parse::<usize>() {
                    current = current.and_then(|n| n.Children.get(idx));
                } else {
                    current = None;
                }
            } else if p.starts_with('+') {
                current = current
                    .and_then(|n| n.Children.iter().find(|n| n.ImageURL == p.replace('+', "")));
            } else {
                current = current.and_then(|n| {
                    n.Children
                        .iter()
                        .find(|n| n.Text.to_lowercase().contains(&p.to_lowercase()))
                });
            }
        }
        current
    }

    fn parse_value_path_def<T: FromStr + Default>(&self, path: &str) -> T {
        self.parse_value_path(path).unwrap_or_default()
    }
}
