#![allow(dead_code)]
/// A single command exposed in the command palette.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub description: String,
    pub keywords: Vec<String>,
}

/// A command together with how well it matched the search query.
#[derive(Debug, Clone)]
pub struct CommandMatch {
    pub command: Command,
    /// Score 0–100, higher is better.
    pub score: u32,
}

/// Fuzzy search over a list of commands.
///
/// Scoring (case-insensitive):
/// - 100  query == label (exact)
/// - 80   label starts_with query
/// - 60   label contains query
/// - 40   any keyword starts_with query
/// - 20   any keyword contains query
/// - 0    no match → excluded from results
///
/// Special case: if `query` is empty every command is returned with score 0,
/// sorted by label ascending.
///
/// Results are sorted by score descending, then label ascending.
pub fn search(commands: &[Command], query: &str) -> Vec<CommandMatch> {
    if query.is_empty() {
        let mut matches: Vec<CommandMatch> = commands
            .iter()
            .map(|c| CommandMatch {
                command: c.clone(),
                score: 0,
            })
            .collect();
        matches.sort_by(|a, b| a.command.label.cmp(&b.command.label));
        return matches;
    }

    let q = query.to_lowercase();

    let mut matches: Vec<CommandMatch> = commands
        .iter()
        .filter_map(|c| {
            let label_lower = c.label.to_lowercase();
            let score = if label_lower == q {
                100
            } else if label_lower.starts_with(&q) {
                80
            } else if label_lower.contains(&q) {
                60
            } else {
                // Check keywords.
                let kw_score = c.keywords.iter().fold(0u32, |best, kw| {
                    let kw_lower = kw.to_lowercase();
                    let s = if kw_lower.starts_with(&q) {
                        40
                    } else if kw_lower.contains(&q) {
                        20
                    } else {
                        0
                    };
                    best.max(s)
                });
                kw_score
            };

            if score > 0 {
                Some(CommandMatch {
                    command: c.clone(),
                    score,
                })
            } else {
                None
            }
        })
        .collect();

    // Sort: score descending, then label ascending.
    matches.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.command.label.cmp(&b.command.label))
    });

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(id: &str, label: &str, description: &str, keywords: &[&str]) -> Command {
        Command {
            id: id.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            keywords: keywords.iter().map(std::string::ToString::to_string).collect(),
        }
    }

    fn sample_commands() -> Vec<Command> {
        vec![
            cmd(
                "open",
                "Open File",
                "Open a file from disk",
                &["file", "load"],
            ),
            cmd(
                "save",
                "Save File",
                "Save the current file",
                &["file", "write"],
            ),
            cmd("quit", "Quit", "Exit the application", &["exit", "close"]),
            cmd(
                "settings",
                "Settings",
                "Open application settings",
                &["preferences", "config"],
            ),
            cmd(
                "muc-join",
                "Join Room",
                "Join a MUC room",
                &["muc", "group", "channel"],
            ),
        ]
    }

    #[test]
    fn exact_match_scores_100() {
        let results = search(&sample_commands(), "quit");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 100);
        assert_eq!(results[0].command.id, "quit");
    }

    #[test]
    fn prefix_match_scores_80() {
        let results = search(&sample_commands(), "sett");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 80);
        assert_eq!(results[0].command.id, "settings");
    }

    #[test]
    fn contains_match_scores_60() {
        // "ile" is contained in both "Open File" and "Save File" but not a prefix.
        let results = search(&sample_commands(), "ile");
        assert!(results.iter().all(|m| m.score == 60));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn keyword_prefix_scores_40() {
        // "conf" is a prefix of keyword "config" in Settings.
        let results = search(&sample_commands(), "conf");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 40);
        assert_eq!(results[0].command.id, "settings");
    }

    #[test]
    fn no_match_excluded() {
        let results = search(&sample_commands(), "zzznomatch");
        assert!(results.is_empty());
    }

    #[test]
    fn results_sorted_by_score_desc() {
        // "file" — exact keyword match for "save" (keyword "file") AND partial label match
        // for both Open File and Save File. But "Open File" and "Save File" contain "file"
        // in the label (score 60), while "file" is an exact keyword for open+save (score 40).
        // Label match wins.
        let results = search(&sample_commands(), "file");
        // All results must be in descending score order.
        for window in results.windows(2) {
            assert!(
                window[0].score >= window[1].score,
                "scores not descending: {} then {}",
                window[0].score,
                window[1].score
            );
        }
    }

    #[test]
    fn empty_query_returns_all_with_zero_score() {
        let cmds = sample_commands();
        let results = search(&cmds, "");
        assert_eq!(results.len(), cmds.len());
        assert!(results.iter().all(|m| m.score == 0));
        // Must be sorted by label ascending.
        let labels: Vec<&str> = results.iter().map(|m| m.command.label.as_str()).collect();
        let mut sorted = labels.clone();
        sorted.sort();
        assert_eq!(labels, sorted);
    }

    #[test]
    fn case_insensitive_matching() {
        let results = search(&sample_commands(), "QUIT");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 100);
    }

    #[test]
    fn keyword_contains_scores_20() {
        // "han" is contained in keyword "channel" of Join Room.
        let results = search(&sample_commands(), "han");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 20);
        assert_eq!(results[0].command.id, "muc-join");
    }
}
