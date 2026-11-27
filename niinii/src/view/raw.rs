use dear_imgui_rs::*;
use ichiran::prelude::*;

use super::mixins::wrap_bullet;

pub struct RawView<'a> {
    root: &'a Root,
}
impl<'a> RawView<'a> {
    pub fn new(root: &'a Root) -> Self {
        Self { root }
    }
    pub fn ui(&mut self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos(0.0);
        add_root(ui, self.root);
    }
}

fn add_root(ui: &Ui, root: &Root) {
    if let Some(_node) = ui.tree_node_config("Root").default_open(true).push() {
        for segment in root.segments() {
            let _id_token = ui.push_id(segment as *const _);
            add_segment(ui, segment);
        }
    }
}
fn add_segment(ui: &Ui, segment: &Segment) {
    match segment {
        Segment::Skipped(_) => wrap_bullet(ui, &format!("{:?}", segment)),
        Segment::Clauses(clauses) => {
            if let Some(_node) = ui.tree_node_config("Segment").default_open(true).push() {
                for clause in clauses {
                    let _id_token = ui.push_id(clause as *const _);
                    add_clause(ui, clause);
                }
            }
        }
    }
}

fn add_clause(ui: &Ui, clause: &Clause) {
    if let Some(_node) = ui
        .tree_node_config(&format!("Clause (score: {})", clause.score()))
        .default_open(true)
        .push()
    {
        for romanized in clause.romanized() {
            let _id_token = ui.push_id(romanized as *const _);
            if let Some(_node) = ui
                .tree_node_config(&format!("Romanized ({})", romanized.romaji()))
                .default_open(true)
                .push()
            {
                add_term(ui, romanized.term());
            }
        }
    }
}
fn add_term(ui: &Ui, term: &Term) {
    match term {
        Term::Word(word) => {
            add_word(ui, word);
        }
        Term::Alternative(alts) => {
            if let Some(_node) = ui.tree_node_config("Alternative").default_open(true).push() {
                for alt in alts.alts() {
                    let _id_token = ui.push_id(alt as *const _);
                    add_word(ui, alt);
                }
            }
        }
    }
}
fn add_word(ui: &Ui, word: &Word) {
    let meta = word.meta();
    if let Some(_node) = ui
        .tree_node_config(&format!("Word ({})", meta.text()))
        .default_open(true)
        .push()
    {
        match word {
            Word::Plain(plain) => {
                add_plain(ui, plain);
            }
            Word::Compound(compound) => {
                add_compound(ui, compound);
            }
        }
    }
}
fn add_plain(ui: &Ui, plain: &Plain) {
    let meta = plain.meta();
    if let Some(_node) = ui
        .tree_node_config(&format!("Plain ({})", meta.reading()))
        .default_open(true)
        .push()
    {
        add_meta(ui, meta);
        if let Some(seq) = plain.seq() {
            wrap_bullet(ui, &format!("seq: {}", seq));
        }
        if let Some(suffix) = plain.suffix() {
            wrap_bullet(ui, &format!("suffix: {}", suffix));
        }
        if let Some(counter) = plain.counter() {
            add_counter(ui, counter);
        }
        if let Some(_node) = ui
            .tree_node_config(&format!("Glosses ({})", plain.gloss().len()))
            .default_open(false)
            .push()
        {
            for gloss in plain.gloss() {
                let _id_token = ui.push_id(gloss as *const _);
                add_gloss(ui, gloss);
            }
        }
        if let Some(_node) = ui
            .tree_node_config(&format!("Conjugations ({})", plain.conj().len()))
            .default_open(false)
            .push()
        {
            for conj in plain.conj() {
                let _id_token = ui.push_id(conj as *const _);
                add_conj(ui, conj);
            }
        }
    }
}
fn add_compound(ui: &Ui, compound: &Compound) {
    let meta = compound.meta();
    if let Some(_node) = ui
        .tree_node_config(&format!("Compound ({})", compound.compound().join(" + ")))
        .default_open(true)
        .push()
    {
        add_meta(ui, meta);
        if let Some(_node) = ui
            .tree_node_config(&format!("Components ({})", compound.components().len()))
            .default_open(false)
            .push()
        {
            for component in compound.components() {
                let _id_token = ui.push_id(component as *const _);
                add_term(ui, component);
            }
        }
    }
}
fn add_meta(ui: &Ui, meta: &Meta) {
    wrap_bullet(ui, &format!("reading: {}", meta.reading()));
    wrap_bullet(ui, &format!("text: {}", meta.text()));
    wrap_bullet(ui, &format!("kana: {}", meta.kana()));
    wrap_bullet(ui, &format!("score: {}", meta.score()));
}
fn add_gloss(ui: &Ui, gloss: &Gloss) {
    if let Some(_node) = ui
        .tree_node_config(&format!("Gloss ({} {})", gloss.pos(), gloss.gloss()))
        .default_open(false)
        .push()
    {
        wrap_bullet(ui, &format!("pos: {}", gloss.pos()));
        wrap_bullet(ui, &format!("gloss: {}", gloss.gloss()));
        if let Some(info) = gloss.info() {
            wrap_bullet(ui, &format!("info: {}", info));
        }
    }
}
fn add_conj(ui: &Ui, conj: &Conjugation) {
    if let Some(_node) = ui.tree_node_config("Conjugation").default_open(true).push() {
        if let Some(reading) = conj.reading() {
            wrap_bullet(ui, &format!("reading: {}", reading));
        }
        if let Some(_node) = ui
            .tree_node_config(&format!("Properties ({})", conj.prop().len()))
            .default_open(false)
            .push()
        {
            for prop in conj.prop() {
                let _id_token = ui.push_id(prop as *const _);
                add_prop(ui, prop);
            }
        }
        if let Some(_node) = ui
            .tree_node_config(&format!("Glosses ({})", conj.gloss().len()))
            .default_open(false)
            .push()
        {
            for gloss in conj.gloss() {
                let _id_token = ui.push_id(gloss as *const _);
                add_gloss(ui, gloss);
            }
        }
        for via in conj.vias() {
            if let Some(_node) = ui.tree_node_config("Via").default_open(false).push() {
                add_conj(ui, via);
            }
        }
        wrap_bullet(ui, &format!("readok: {}", conj.readok()));
    }
}
fn add_prop(ui: &Ui, prop: &Property) {
    wrap_bullet(
        ui,
        &format!(
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
        &format!(
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
