//! Multilingual semantic-embedding backend Cargo feature `semantic`.
//!
//! Loads a BERT-architecture sentence model (config.json + tokenizer.json +
//! model.safetensors) from `$LUMA_EMBED_MODEL_DIR` (default `./models/ml-minilm`)
//! and mean-pools its token embeddings into a 384-d vector. CPU only.
//!
//! Chosen model: **`paraphrase-multilingual-MiniLM-L12-v2`** (384-d, 12 layers,
//! ~470 MB). Libraries are often non-English (ours has French overviews), and an
//! English-only model (`all-MiniLM-L6-v2`) collapsed every query returned the
//! same handful of items. A *multilingual* model maps FR docs + EN/FR queries into
//! one space; verified on the live library (christmas → Nightmare Before Christmas
//! / Elf / Krampus; heist → Heat / Le Cercle rouge). Keep the phrase bank in
//! ENGLISH English queries outperform French against this model.
//!
//! Build notes (musl / Synology): keep candle on its pure-Rust `gemm` backend (no
//! mkl/accelerate/cuda) and `tokenizers` with `onig` (Oniguruma C cross-compiles
//! like the bundled SQLite/zstd C). Requires rustc ≥ 1.85 (edition2024); see the
//! `semantic` feature comment in Cargo.toml. Any BERT-arch sentence
//! model in the dir works, but `dim()` assumes 384-d output.

use std::path::PathBuf;

use anyhow::{Context, Result};
use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use tokenizers::Tokenizer;

use super::Embedder;

pub struct MiniLmEmbedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl MiniLmEmbedder {
    /// Load the model from disk once. Ship the three files alongside the binary
    /// (or `include_bytes!` them) so the NAS install is self-contained.
    pub fn load() -> Result<Self> {
        let dir = std::env::var("LUMA_EMBED_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("models/ml-minilm"));
        let device = Device::Cpu;

        let config: Config = serde_json::from_str(
            &std::fs::read_to_string(dir.join("config.json")).context("read config.json")?,
        )
        .context("parse config.json")?;

        let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| anyhow::anyhow!("load tokenizer.json: {e}"))?;

        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[dir.join("model.safetensors")], DTYPE, &device)
                .context("mmap safetensors")?
        };
        let model = BertModel::load(vb, &config).context("build BERT")?;

        Ok(Self { model, tokenizer, device })
    }

    fn embed_inner(&self, text: &str) -> Result<Vec<f32>> {
        let enc = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("tokenize: {e}"))?;
        let ids = enc.get_ids();

        // [1, seq_len] a single, unpadded sequence.
        let tokens = Tensor::new(ids, &self.device)?.unsqueeze(0)?;
        let token_type = tokens.zeros_like()?;
        let mask = tokens.ones_like()?;

        // [1, seq_len, 384] → mean-pool over the sequence → [384].
        let out = self.model.forward(&tokens, &token_type, Some(&mask))?;
        let pooled = out.mean(1)?.squeeze(0)?;
        let mut v: Vec<f32> = pooled.to_vec1()?;
        super::l2_normalize(&mut v);
        Ok(v)
    }
}

impl Embedder for MiniLmEmbedder {
    fn dim(&self) -> usize {
        384
    }

    fn relevance_floor(&self) -> f32 {
        // Calibrated on the live library with paraphrase-multilingual-MiniLM-L12:
        // genuine themed matches score ~0.45–0.66, so ~0.38 keeps them while a
        // query the library can't satisfy (best hit below it) drops the row.
        0.38
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        // Never poison the enrichment pass on a single bad input: log and return
        // a zero vector (which scores 0 against everything and is simply ignored).
        match self.embed_inner(text) {
            Ok(v) => v,
            Err(err) => {
                tracing::warn!(error = %err, "MiniLM embed failed; using zero vector");
                vec![0.0; self.dim()]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Embedder;

    /// End-to-end runtime check: loads the real model and verifies the output is a
    /// 384-d unit vector AND that it ranks *meaning*, not words the whole reason
    /// for this backend. Needs the model files on disk, so it's ignored by default:
    ///   LUMA_EMBED_MODEL_DIR=models/minilm \
    ///     cargo test -p luma-vector --features semantic minilm -- --ignored --nocapture
    #[test]
    #[ignore = "needs the MiniLM model files (models/minilm) on disk"]
    fn minilm_embeds_and_ranks_by_meaning() {
        let e = MiniLmEmbedder::load().expect("load MiniLM model");
        assert_eq!(e.dim(), 384);

        let christmas = e.embed("a heartwarming christmas movie about santa and family");
        let holiday = e.embed("a festive holiday film with father christmas giving gifts");
        let war = e.embed("a brutal documentary about tank warfare and combat");

        // L2-normalized (so cosine == dot downstream).
        let norm = christmas.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert_eq!(christmas.len(), 384);
        assert!((norm - 1.0).abs() < 1e-3, "expected unit length, got {norm}");

        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        let near = dot(&christmas, &holiday); // same vibe, different words
        let far = dot(&christmas, &war); // different vibe
        // Semantics, not keywords: christmas≈holiday must clearly beat christmas≈war.
        assert!(near > far + 0.1, "christmas~holiday {near:.3} should ≫ christmas~war {far:.3}");
    }

    /// Non-destructive A/B on the LIVE library: embeds every enriched title with
    /// MiniLM **in memory** (no DB writes, no re-scan, no server restart) and
    /// prints the top hits per themed query, with scores so we can compare to
    /// the lexical `/themed` probes and calibrate the MiniLM relevance floor.
    ///   LUMA_EMBED_MODEL_DIR=models/minilm cargo test -p luma-vector --features semantic \
    ///     minilm_library_probe -- --ignored --nocapture
    #[test]
    #[ignore = "manual: reads data/luma.db + model, prints MiniLM rankings on the real library"]
    fn minilm_library_probe() {
        let e = MiniLmEmbedder::load().expect("load MiniLM model");
        let conn = rusqlite::Connection::open("data/luma.db").expect("open data/luma.db");
        let mut stmt = conn
            .prepare("SELECT title, year, metadata FROM items WHERE kind != 'episode' AND metadata IS NOT NULL")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .unwrap();

        let mut lib: Vec<(String, Vec<f32>)> = Vec::new();
        for row in rows {
            let (title, year, meta_json) = row.unwrap();
            let Some(mj) = meta_json else { continue };
            let Ok(meta) = serde_json::from_str::<luma_module_sdk::domain::metadata::Metadata>(&mj) else { continue };
            let doc = luma_module_sdk::domain::build_doc(&title, year.map(|y| y as u32), &meta);
            lib.push((title, e.embed(&doc)));
        }
        eprintln!("\nMiniLM-embedded {} titles from the live library\n", lib.len());

        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();

        // Corpus mean embedding the anisotropy "common direction" we subtract.
        let dim = lib[0].1.len();
        let mut mean = vec![0f32; dim];
        for (_, v) in &lib {
            for (m, x) in mean.iter_mut().zip(v) {
                *m += x;
            }
        }
        for m in mean.iter_mut() {
            *m /= lib.len() as f32;
        }
        // Center against the corpus mean, then re-normalize.
        let center = |v: &[f32]| -> Vec<f32> {
            let mut c: Vec<f32> = v.iter().zip(&mean).map(|(x, m)| x - m).collect();
            let n = c.iter().map(|x| x * x).sum::<f32>().sqrt();
            if n > 0.0 {
                for x in c.iter_mut() {
                    *x /= n;
                }
            }
            c
        };
        let lib_c: Vec<(&str, Vec<f32>)> =
            lib.iter().map(|(t, v)| (t.as_str(), center(v))).collect();

        let top = |scored: &mut Vec<(&str, f32)>| -> String {
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            scored.iter().take(8).map(|(t, s)| format!("{t} ({s:.2})")).collect::<Vec<_>>().join("  ·  ")
        };

        // EN/FR pairs so we can see whether a multilingual model bridges the
        // English-query → French-overview gap, and whether French queries do better.
        let probes = [
            "christmas holiday movie",
            "film de noël féérique en famille",
            "clever heist crew robbery thriller",
            "film de braquage avec une équipe",
            "feel-good uplifting comedy",
            "comédie légère qui fait du bien",
        ];
        for q in probes {
            let qv = e.embed(q);
            let qc = center(&qv);
            let mut raw: Vec<(&str, f32)> = lib.iter().map(|(t, v)| (t.as_str(), dot(v, &qv))).collect();
            let mut cen: Vec<(&str, f32)> = lib_c.iter().map(|(t, v)| (*t, dot(v, &qc))).collect();
            eprintln!("── \"{q}\"\n   RAW      {}\n   CENTERED {}\n", top(&mut raw), top(&mut cen));
        }
    }
}
