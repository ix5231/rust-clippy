use crate::utils::{differing_macro_contexts, paths, snippet_with_applicability, span_lint_and_then};
use crate::utils::{is_copy, match_type};
use rustc::hir::intravisit::{walk_path, NestedVisitorMap, Visitor};
use rustc::hir::{self, *};
use rustc::lint::LateContext;
use rustc_data_structures::fx::FxHashSet;
use rustc_errors::Applicability;
use rustc_span::source_map::Span;
use rustc_span::symbol::Symbol;

use super::OPTION_MAP_UNWRAP_OR;

/// lint use of `map().unwrap_or()` for `Option`s
pub(super) fn lint<'a, 'tcx>(
    cx: &LateContext<'a, 'tcx>,
    expr: &hir::Expr<'_>,
    map_args: &'tcx [hir::Expr<'_>],
    unwrap_args: &'tcx [hir::Expr<'_>],
    map_span: Span,
) {
    // lint if the caller of `map()` is an `Option`
    if match_type(cx, cx.tables.expr_ty(&map_args[0]), &paths::OPTION) {
        if !is_copy(cx, cx.tables.expr_ty(&unwrap_args[1])) {
            // Do not lint if the `map` argument uses identifiers in the `map`
            // argument that are also used in the `unwrap_or` argument

            let mut unwrap_visitor = UnwrapVisitor {
                cx,
                identifiers: FxHashSet::default(),
            };
            unwrap_visitor.visit_expr(&unwrap_args[1]);

            let mut map_expr_visitor = MapExprVisitor {
                cx,
                identifiers: unwrap_visitor.identifiers,
                found_identifier: false,
            };
            map_expr_visitor.visit_expr(&map_args[1]);

            if map_expr_visitor.found_identifier {
                return;
            }
        }

        if differing_macro_contexts(unwrap_args[1].span, map_span) {
            return;
        }

        let mut applicability = Applicability::MachineApplicable;
        // get snippet for unwrap_or()
        let unwrap_snippet = snippet_with_applicability(cx, unwrap_args[1].span, "..", &mut applicability);
        // lint message
        // comparing the snippet from source to raw text ("None") below is safe
        // because we already have checked the type.
        let arg = if unwrap_snippet == "None" { "None" } else { "a" };
        let unwrap_snippet_none = unwrap_snippet == "None";
        let suggest = if unwrap_snippet_none {
            "and_then(f)"
        } else {
            "map_or(a, f)"
        };
        let msg = &format!(
            "called `map(f).unwrap_or({})` on an Option value. \
             This can be done more directly by calling `{}` instead",
            arg, suggest
        );

        span_lint_and_then(cx, OPTION_MAP_UNWRAP_OR, expr.span, msg, |db| {
            let map_arg_span = map_args[1].span;

            let mut suggestion = vec![
                (
                    map_span,
                    String::from(if unwrap_snippet_none { "and_then" } else { "map_or" }),
                ),
                (expr.span.with_lo(unwrap_args[0].span.hi()), String::from("")),
            ];

            if !unwrap_snippet_none {
                suggestion.push((map_arg_span.with_hi(map_arg_span.lo()), format!("{}, ", unwrap_snippet)));
            }

            db.multipart_suggestion(&format!("use `{}` instead", suggest), suggestion, applicability);
        });
    }
}

struct UnwrapVisitor<'a, 'tcx> {
    cx: &'a LateContext<'a, 'tcx>,
    identifiers: FxHashSet<Symbol>,
}

impl<'a, 'tcx> Visitor<'tcx> for UnwrapVisitor<'a, 'tcx> {
    fn visit_path(&mut self, path: &'tcx Path<'_>, _id: HirId) {
        self.identifiers.insert(ident(path));
        walk_path(self, path);
    }

    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::All(&self.cx.tcx.hir())
    }
}

struct MapExprVisitor<'a, 'tcx> {
    cx: &'a LateContext<'a, 'tcx>,
    identifiers: FxHashSet<Symbol>,
    found_identifier: bool,
}

impl<'a, 'tcx> Visitor<'tcx> for MapExprVisitor<'a, 'tcx> {
    fn visit_path(&mut self, path: &'tcx Path<'_>, _id: HirId) {
        if self.identifiers.contains(&ident(path)) {
            self.found_identifier = true;
            return;
        }
        walk_path(self, path);
    }

    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::All(&self.cx.tcx.hir())
    }
}

fn ident(path: &Path<'_>) -> Symbol {
    path.segments
        .last()
        .expect("segments should be composed of at least 1 element")
        .ident
        .name
}
