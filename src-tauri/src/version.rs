// SPDX-License-Identifier: Apache-2.0
//! Versions as the OpenStage suite writes them: MAJOR.MINOR.PATCH, or with -alpha.N, -beta.N or -rc.N after it. A final release sorts after its
//! pre-releases. The same rule as Check for updates in each OpenStage app (OpenExplainer's src-tauri/src/updates.rs).

pub type Key = (u64, u64, u64, u8, u64);

/// A version as it sorts: the three numbers, then where it stands (alpha, beta, rc, final) and its number.
pub fn key(text: &str) -> Option<Key> {
    let text = text.trim().trim_start_matches('v');
    let (main, pre) = match text.split_once('-') {
        Some((m, p)) => (m, Some(p)),
        None => (text, None),
    };
    let number = |n: &str| if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) { None } else { n.parse::<u64>().ok() };
    let mut nums = main.split('.').map(number);
    let (a, b, c) = (nums.next()??, nums.next()??, nums.next()??);
    if nums.next().is_some() {
        return None;
    }
    let (stage, n) = match pre {
        None => (3, 0),
        Some(p) => {
            let (word, num) = p.split_once('.')?;
            let stage = match word {
                "alpha" => 0,
                "beta" => 1,
                "rc" => 2,
                _ => return None,
            };
            (stage, number(num)?)
        }
    };
    Some((a, b, c, stage, n))
}

/// The three numbers only (0.2.0 for 0.2.0-alpha.4): what Windows' installed apps list shows for an OpenStage app.
pub fn base(text: &str) -> Option<(u64, u64, u64)> {
    let k = key(text.split_once('-').map_or(text, |(m, _)| m))?;
    Some((k.0, k.1, k.2))
}

pub fn is_preview(text: &str) -> bool {
    key(text).is_some_and(|k| k.3 < 3)
}

/// Whether `a` is newer than `b`. A version that cannot be read is never newer.
pub fn newer(a: &str, b: &str) -> bool {
    matches!((key(a), key(b)), (Some(x), Some(y)) if x > y)
}

/// "0.2.0 alpha 4" for 0.2.0-alpha.4.
pub fn spoken(version: &str) -> String {
    let v = version.trim_start_matches('v');
    match v.split_once('-') {
        Some((main, pre)) => format!("{main} {}", pre.replace('.', " ")),
        None => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_sort_with_pre_releases_before_the_final_one() {
        let order = ["0.1.0-alpha.1", "0.1.0-alpha.2", "0.1.0-alpha.10", "0.1.0-beta.1", "0.1.0-rc.1", "0.1.0", "0.1.1-alpha.1", "0.2.0", "1.0.0"];
        for pair in order.windows(2) {
            assert!(newer(pair[1], pair[0]), "{} < {}", pair[0], pair[1]);
            assert!(!newer(pair[0], pair[1]));
        }
        for bad in ["", "0.1", "0.1.0.1", "0.1.0-gamma.1", "0.1.0-alpha", "x.1.0"] {
            assert_eq!(key(bad), None, "{bad}");
            assert!(!newer(bad, "0.0.1"));
        }
    }

    #[test]
    fn the_base_and_the_spoken_form() {
        assert_eq!(base("0.2.0-alpha.4"), Some((0, 2, 0)));
        assert_eq!(base("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(base("1.2"), None);
        assert_eq!(spoken("0.1.0-alpha.23"), "0.1.0 alpha 23");
        assert!(is_preview("0.1.0-rc.1"));
        assert!(!is_preview("0.1.0"));
    }
}
