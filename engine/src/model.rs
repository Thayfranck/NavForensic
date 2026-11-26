//! Modele de donnees serialisable (envoye tel quel au frontend React).

use serde::Serialize;
use std::collections::HashMap;

/// Une cellule de tableau. `Time` est un horodatage en millisecondes Unix UTC
/// (le frontend le formate en UTC ou en heure locale).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "t", content = "v")]
pub enum Cell {
    #[serde(rename = "text")]
    Text(String),
    #[serde(rename = "int")]
    Int(i64),
    #[serde(rename = "time")]
    Time(i64),
    #[serde(rename = "null")]
    Null,
}

impl Cell {
    pub fn text<S: Into<String>>(s: S) -> Cell {
        let s = s.into();
        if s.is_empty() {
            Cell::Null
        } else {
            Cell::Text(s)
        }
    }
    pub fn opt_text(s: Option<String>) -> Cell {
        match s {
            Some(v) if !v.is_empty() => Cell::Text(v),
            _ => Cell::Null,
        }
    }
    pub fn opt_int(v: Option<i64>) -> Cell {
        match v {
            Some(n) => Cell::Int(n),
            None => Cell::Null,
        }
    }
    pub fn opt_time(ms: Option<i64>) -> Cell {
        match ms {
            Some(n) => Cell::Time(n),
            None => Cell::Null,
        }
    }
    /// Valeur de tri / recherche en texte brut.
    pub fn as_search(&self) -> String {
        match self {
            Cell::Text(s) => s.clone(),
            Cell::Int(n) => n.to_string(),
            Cell::Time(ms) => ms.to_string(),
            Cell::Null => String::new(),
        }
    }
    pub fn time_ms(&self) -> Option<i64> {
        match self {
            Cell::Time(ms) => Some(*ms),
            _ => None,
        }
    }

    pub fn int_val(&self) -> Option<i64> {
        match self {
            Cell::Int(n) => Some(*n),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Row {
    pub values: HashMap<String, Cell>,
    /// Valeurs brutes non converties (pour la fenetre de detail).
    pub raw: Vec<(String, String)>,
}

impl Row {
    pub fn new() -> Row {
        Row::default()
    }
    pub fn set<S: Into<String>>(&mut self, col: S, cell: Cell) {
        self.values.insert(col.into(), cell);
    }
    pub fn raw<S: Into<String>, T: Into<String>>(&mut self, k: S, v: T) {
        self.raw.push((k.into(), v.into()));
    }
    pub fn get(&self, col: &str) -> Option<&Cell> {
        self.values.get(col)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Category {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

impl Category {
    pub fn new(name: &str, columns: &[&str], rows: Vec<Row>) -> Category {
        Category {
            name: name.to_string(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Artifact {
    pub evidence: String,
    pub path: String,
    pub sha256: String,
    pub browser: String,
    pub kind: String,
    pub categories: Vec<Category>,
    pub notes: Vec<String>,
}
