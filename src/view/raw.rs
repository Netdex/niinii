use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use ichiran::types::*;
use imgui::*;

use crate::support::Env;

fn id<T: Hash>(x: T) -> i32 {
    let mut hasher = DefaultHasher::new();
    x.hash(&mut hasher);
    hasher.finish() as i32
}

fn wrap_bullet(ui: &Ui, text: &ImStr) {
    ui.bullet();
    ui.text_wrapped(text);
}

pub struct RawView<'a> {
    root: &'a Root,
}
impl<'a> RawView<'a> {
    pub fn new(root: &'a Root) -> Self {
        Self { root }
    }
    pub fn ui(&mut self, _env: &mut Env, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos();
        add_root(ui, self.root);
    }
}

fn add_root(ui: &Ui, root: &Root) {
    TreeNode::new(im_str!("Root"))
        .default_open(true)
        .build(ui, || {
            for segment in root.segments() {
                let _id_token = ui.push_id(id(segment));
                add_segment(ui, segment);
            }
        });
}
fn add_segment(ui: &Ui, segment: &Segment) {
    match segment {
        Segment::Skipped(_) => wrap_bullet(ui, &im_str!("{:?}", segment)),
        Segment::Clauses(clauses) => {
            TreeNode::new(im_str!("Clauses"))
                .default_open(true)
                .build(ui, || {
                    for clause in clauses {
                        let _id_token = ui.push_id(id(clause));
                        add_clause(ui, clause);
                    }
                });
        }
    }
}
fn add_clause(ui: &Ui, clause: &Clause) {
    TreeNode::new(&im_str!("Clause (score: {})", clause.score()))
        .default_open(true)
        .build(ui, || {
            for romanized in clause.romanized() {
                let _id_token = ui.push_id(id(romanized));
                TreeNode::new(&im_str!("Romanized ({})", romanized.romaji()))
                    .default_open(true)
                    .build(ui, || {
                        add_term(ui, romanized.term());
                    });
            }
        });
}
fn add_term(ui: &Ui, term: &Term) {
    match term {
        Term::Word(word) => {
            add_word(ui, word);
        }
        Term::Alternative(alts) => {
            TreeNode::new(im_str!("Alternative"))
                .default_open(true)
                .build(ui, || {
                    for alt in alts.alts() {
                        let _id_token = ui.push_id(id(alt));
                        add_word(ui, alt);
                    }
                });
        }
    }
}
fn add_word(ui: &Ui, word: &Word) {
    let meta = word.meta();
    TreeNode::new(&im_str!("Word ({})", meta.text()))
        .default_open(true)
        .build(ui, || match word {
            Word::Plain(plain) => {
                add_plain(ui, plain);
            }
            Word::Compound(compound) => {
                add_compound(ui, compound);
            }
        });
}
fn add_plain(ui: &Ui, plain: &Plain) {
    let meta = plain.meta();
    TreeNode::new(&im_str!("Plain ({})", meta.reading()))
        .default_open(true)
        .build(ui, || {
            add_meta(ui, meta);
            if let Some(seq) = plain.seq() {
                wrap_bullet(ui, &im_str!("seq: {}", seq));
            }
            if let Some(suffix) = plain.suffix() {
                wrap_bullet(ui, &im_str!("suffix: {}", suffix));
            }
            if let Some(counter) = plain.counter() {
                add_counter(ui, counter);
            }
            TreeNode::new(&im_str!("Glosses ({})", plain.gloss().len()))
                .default_open(false)
                .build(ui, || {
                    for gloss in plain.gloss() {
                        let _id_token = ui.push_id(id(gloss));
                        add_gloss(ui, gloss);
                    }
                });
            TreeNode::new(&im_str!("Conjugations ({})", plain.conj().len()))
                .default_open(false)
                .build(ui, || {
                    for conj in plain.conj() {
                        let _id_token = ui.push_id(id(conj));
                        add_conj(ui, conj);
                    }
                });
        });
}
fn add_compound(ui: &Ui, compound: &Compound) {
    let meta = compound.meta();
    TreeNode::new(&im_str!("Compound ({})", compound.compound().join(" + ")))
        .default_open(true)
        .build(ui, || {
            add_meta(ui, meta);
            TreeNode::new(&im_str!("Components ({})", compound.components().len()))
                .default_open(false)
                .build(ui, || {
                    for component in compound.components() {
                        let _id_token = ui.push_id(id(component));
                        add_term(ui, component);
                    }
                });
        });
}
fn add_meta(ui: &Ui, meta: &Meta) {
    wrap_bullet(ui, &im_str!("reading: {}", meta.reading()));
    wrap_bullet(ui, &im_str!("text: {}", meta.text()));
    wrap_bullet(ui, &im_str!("kana: {}", meta.kana()));
    wrap_bullet(ui, &im_str!("score: {}", meta.score()));
}
fn add_gloss(ui: &Ui, gloss: &Gloss) {
    TreeNode::new(&im_str!("Gloss ({} {})", gloss.pos(), gloss.gloss()))
        .default_open(false)
        .build(ui, || {
            wrap_bullet(ui, &im_str!("pos: {}", gloss.pos()));
            wrap_bullet(ui, &im_str!("gloss: {}", gloss.gloss()));
            if let Some(info) = gloss.info() {
                wrap_bullet(ui, &im_str!("info: {}", info));
            }
        });
}
fn add_conj(ui: &Ui, conj: &Conjugation) {
    TreeNode::new(&im_str!("Conjugation"))
        .default_open(true)
        .build(ui, || {
            if let Some(reading) = conj.reading() {
                wrap_bullet(ui, &im_str!("reading: {}", reading));
            }
            TreeNode::new(&im_str!("Properties ({})", conj.prop().len()))
                .default_open(false)
                .build(ui, || {
                    for prop in conj.prop() {
                        let _id_token = ui.push_id(id(prop));
                        add_prop(ui, prop);
                    }
                });
            TreeNode::new(&im_str!("Glosses ({})", conj.gloss().len()))
                .default_open(false)
                .build(ui, || {
                    for gloss in conj.gloss() {
                        let _id_token = ui.push_id(id(gloss));
                        add_gloss(ui, gloss);
                    }
                });
            if let Some(via) = conj.via() {
                TreeNode::new(&im_str!("Via ({})", conj.via().is_some()))
                    .default_open(false)
                    .build(ui, || {
                        add_conj(ui, via);
                    });
            }

            wrap_bullet(ui, &im_str!("readok: {}", conj.readok()));
        });
}
fn add_prop(ui: &Ui, prop: &Property) {
    wrap_bullet(
        ui,
        &im_str!(
            "Property ([{}] {} {} {})",
            prop.pos(),
            prop.kind(),
            if prop.fml() { "fml" } else { "non-fml" },
            if prop.neg() { "neg" } else { "non-neg" }
        ),
    );
}
fn add_counter(ui: &Ui, counter: &Counter) {
    wrap_bullet(
        ui,
        &im_str!(
            "Counter ({} {})",
            counter.value(),
            if counter.ordinal() {
                "ordinal"
            } else {
                "non-ordinal"
            }
        ),
    );
}
