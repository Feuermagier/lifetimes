mod checker;
mod polonius_checker;

use std::{borrow::BorrowMut, collections::HashMap, fmt::Debug, sync::Arc};

use base_db::{CrateOrigin, Env};
use checker::{VarId, Vars};
use hir::{db::HirDatabase, CfgOptions, Semantics};
use ide::{AnalysisHost, Change, CrateGraph, Edition, FileId, SourceRoot};
use syntax::AstToken;
use syntax::{
    ast::{self, AstNode},
    Direction,
};

use vfs::{file_set::FileSet, VfsPath};

fn main() {
    //let subscriber = tracing_subscriber::FmtSubscriber::builder().with_max_level(LevelFilter::DEBUG).finish();
    //tracing::subscriber::set_global_default(subscriber).unwrap();

    let mut host = AnalysisHost::new(None);

    let mut file_set = FileSet::default();
    let file = FileId(0);
    file_set.insert(file, VfsPath::new_virtual_path("/main.rs".to_string()));
    let root = SourceRoot::new_local(file_set);

    let mut crate_graph = CrateGraph::default();
    crate_graph.add_crate_root(
        file,
        Edition::Edition2021,
        None,
        None,
        CfgOptions::default(),
        CfgOptions::default(),
        Env::default(),
        Vec::new(),
        CrateOrigin::Unknown,
    );

    let mut initial_change = Change::default();
    initial_change.set_roots(vec![root]);
    initial_change.set_crate_graph(crate_graph);
    initial_change.change_file(
        FileId(0),
        Some(Arc::new(
            r#"
fn main() {
    let mut x = 3;
    let y = &mut x;
    *y = 4;
    let z = &*y;
}
"#
            .to_string(),
        )),
    );
    host.apply_change(initial_change);

    let analysis = host.analysis();

    let offset = analysis
        .file_line_index(file)
        .unwrap()
        .offset(ide::LineCol { line: 2, col: 12 });

    let semantics = Semantics::new(host.raw_database());
    let file_node = semantics.parse(file);
    let ast = file_node.syntax();

    let token = ast.token_at_offset(offset);
    let token = match token {
        syntax::TokenAtOffset::None => todo!(),
        syntax::TokenAtOffset::Single(_) => todo!(),
        syntax::TokenAtOffset::Between(lhs, rhs) => rhs,
    };

    dbg!(ast);

    let mut locals_map = HashMap::new();
    let mut vars = Vars::new();

    /*
    let local = semantics
        .to_def(&token.ancestors().find_map(ast::IdentPat::cast).unwrap())
        .unwrap();
    let var = vars.create_var(local, &semantics);
    locals_map.insert(local, var);
    */

    let parent_stmt = token.ancestors().find_map(ast::Stmt::cast).unwrap();

    for stmt in parent_stmt
        .syntax()
        .siblings(Direction::Next)
        .filter_map(ast::Stmt::cast)
    {
        process_statement(&stmt, &mut vars, &mut locals_map, &semantics);
        println!("{}", vars);
    }
}

fn process_statement<'db, DB: HirDatabase>(
    stmt: &ast::Stmt,
    vars: &mut Vars,
    locals_map: &mut HashMap<hir::Local, VarId>,
    sema: &Semantics<'db, DB>,
) {
    println!("\n\nProcessing '{}'", stmt.syntax().text());
    match stmt {
        ast::Stmt::ExprStmt(expr) => match expr.expr().unwrap() {
            ast::Expr::BinExpr(bin_expr) => {
                let lhs = bin_expr.lhs().unwrap();
                let rhs = bin_expr.rhs().unwrap();
                match &lhs {
                    ast::Expr::PathExpr(path) => {
                        let local = resolve_local_ref(path.path().unwrap(), sema).unwrap();
                        let lhs_var = *locals_map.get(&local).unwrap();
                        let rhs_var = resolve_borrow_target(&rhs, vars, locals_map, sema);
                        vars.resolve_var(lhs_var)
                            .borrow_mut()
                            .transition_initialized(rhs_var, &vars);
                    }
                    ast::Expr::PrefixExpr(prefix_expr) => {
                        if prefix_expr.op_kind().unwrap() != ast::UnaryOp::Deref {
                            todo!();
                        }
                        match &prefix_expr.expr().unwrap() {
                            ast::Expr::PathExpr(path) => {
                                let local = resolve_local_ref(path.path().unwrap(), sema).unwrap();
                                let lhs_inner_var = *locals_map.get(&local).unwrap();
                                let lhs_var = vars.get_deref_var(lhs_inner_var);
                                let rhs_var = resolve_borrow_target(&rhs, vars, locals_map, sema);
                                vars.resolve_var(lhs_var)
                                    .borrow_mut()
                                    .transition_initialized(rhs_var, &vars);
                            }
                            _ => todo!(),
                        }
                    }
                    _ => todo!(),
                }
            }
            _ => todo!(),
        },
        ast::Stmt::LetStmt(let_stmt) => {
            if let ast::Pat::IdentPat(ident) = let_stmt.pat().unwrap() {
                let new_local = sema.to_def(&ident).unwrap();
                /*
                let is_copy = sema
                    .type_of_pat(&ident.pat().unwrap())
                    .unwrap()
                    .original()
                    .is_copy(sema.db);
                    */
                let new_var = vars.create_var(
                    ident.mut_token().is_some(),
                    false, // TODO
                    new_local
                        .name(sema.db)
                        .unwrap()
                        .as_text()
                        .unwrap()
                        .to_string(),
                );
                locals_map.insert(new_local, new_var);

                if let Some(init) = let_stmt.initializer() {
                    let rhs = resolve_borrow_target(&init, vars, locals_map, sema);
                    vars.resolve_var(new_var)
                        .borrow_mut()
                        .transition_initialized(rhs, &vars);
                }
            } else {
                todo!();
            }
        }
        ast::Stmt::Item(_) => todo!(),
    }
}

fn resolve_borrow_target<'db, DB: HirDatabase>(
    expr: &ast::Expr,
    vars: &mut Vars,
    locals_map: &HashMap<hir::Local, VarId>,
    sema: &Semantics<'db, DB>,
) -> VarId {
    match expr {
        ast::Expr::Literal(literal) => vars.create_literal(literal.syntax().text().to_string()),
        ast::Expr::PathExpr(path) => {
            let local = resolve_local_ref(path.path().unwrap(), sema).unwrap();
            *locals_map.get(&local).unwrap()
            /*
            let mut target = vars
                .resolve_var(*locals_map.get(&local).unwrap())
                .borrow_mut();

            if is_mut_borrow {
                target.transition_mut_borrowed(borrower, vars);
            } else {
                target.transition_borrowed(borrower, vars);
            }
            */
        }
        ast::Expr::RefExpr(subexpr) => {
            let is_mut_borrow = subexpr.mut_token().is_some();

            let tmp =
                vars.create_ref_tmp(is_mut_borrow, subexpr.syntax().text().to_string()); // Dunno if it is correct that a tmp var created by an immutable borrow is immutable

            let target = resolve_borrow_target(&subexpr.expr().unwrap(), vars, locals_map, sema);
            let mut target = vars.resolve_var(target).borrow_mut();

            if is_mut_borrow {
                target.transition_mut_borrowed(tmp, vars);
            } else {
                target.transition_borrowed(tmp, vars);
            }

            tmp
        }
        ast::Expr::PrefixExpr(prefix_expr) => {
            if prefix_expr.op_kind().unwrap() != ast::UnaryOp::Deref {
                todo!();
            }
            let target =
                resolve_borrow_target(&prefix_expr.expr().unwrap(), vars, locals_map, sema);

            vars.get_deref_var(target)
        }
        _ => todo!(),
    }
}

fn resolve_local_ref<'db, DB: HirDatabase>(
    path: ast::Path,
    sema: &Semantics<'db, DB>,
) -> Option<hir::Local> {
    match sema.resolve_path(&path).unwrap() {
        hir::PathResolution::Local(local) => Some(local),
        _ => None,
    }
}
