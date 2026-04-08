//! Emits `GrammarBuilder` parse-context flag checks for version and experimental gates.

use crate::parse::version::{
    FLAG_EXP_EXCEPTIONS, FLAG_EXP_GOTO, FLAG_EXP_LET, FLAG_EXP_LEXICAL_CONST, FLAG_EXP_LOOP_LEVELS,
    FLAG_EXP_MATCH, FLAG_EXP_MODULES, FLAG_EXP_TEMPLATES, FLAG_V1, FLAG_V2, FLAG_V3, FLAG_V4,
};
use sipha::prelude::*;

pub(crate) fn v2(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_V2);
}

pub(crate) fn v3(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_V3);
}

pub(crate) fn v4(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_V4);
}

pub(crate) fn not_v1(g: &mut GrammarBuilder) {
    g.if_not_flag(FLAG_V1);
}

pub(crate) fn exp_let(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_EXP_LET);
}

pub(crate) fn exp_lexical_const(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_EXP_LEXICAL_CONST);
}

pub(crate) fn exp_match(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_EXP_MATCH);
}

pub(crate) fn exp_modules(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_EXP_MODULES);
}

pub(crate) fn exp_exceptions(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_EXP_EXCEPTIONS);
}

pub(crate) fn exp_goto(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_EXP_GOTO);
}

pub(crate) fn exp_loop_levels(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_EXP_LOOP_LEVELS);
}

pub(crate) fn not_exp_loop_levels(g: &mut GrammarBuilder) {
    g.if_not_flag(FLAG_EXP_LOOP_LEVELS);
}

pub(crate) fn exp_templates(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_EXP_TEMPLATES);
}
