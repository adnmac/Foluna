//! Texte de la notification envoyée quand la file d'attente part sur la liseuse branchée (`SendSummary.swift`).
//! Singulier et pluriel sont des textes distincts : chaque langue les traduit.

pub const TITLE: &str = "Livres envoyés sur la liseuse";

/// Corps de la notification, ou None s'il n'y a rien à dire.
pub fn body(sent: &[String], failed: usize, reader: Option<&str>) -> Option<String> {
    match (sent.len(), failed) {
        (0, 0) => None,
        (0, 1) => Some("Échec de l'envoi de 1 livre".into()),
        (0, f) => Some(format!("Échec de l'envoi de {f} livres")),
        (1, 0) => Some(match reader {
            Some(r) => format!("« {} » envoyé sur « {r} »", sent[0]),
            None => format!("« {} » envoyé sur la liseuse", sent[0]),
        }),
        (n, 0) => Some(match reader {
            Some(r) => format!("{n} livres envoyés sur « {r} »"),
            None => format!("{n} livres envoyés sur la liseuse"),
        }),
        (n, f) => {
            let books = if n == 1 { "1 livre envoyé".to_string() } else { format!("{n} livres envoyés") };
            let failures = if f == 1 { "1 échec".to_string() } else { format!("{f} échecs") };
            Some(format!("{books}, {failures}"))
        }
    }
}
