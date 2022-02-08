mod checker;
mod polonius_checker;

use std::{collections::HashMap, fmt::Debug, sync::Arc};

use base_db::{CrateOrigin, Env};
use checker::{Checker, CheckerError, VarId};
use hir::{db::HirDatabase, CfgOptions, Semantics};
use ide::{AnalysisHost, Change, CrateGraph, Edition, FileId, SourceRoot};
use log::info;
use syntax::ast::{self, AstNode, HasName};

use vfs::{file_set::FileSet, VfsPath};

pub fn check(code: String) -> Result<(), CheckError> {
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
    initial_change.change_file(FileId(0), Some(Arc::new(code.to_string())));
    host.apply_change(initial_change);

    let semantics = Semantics::new(host.raw_database());
    let file_node = semantics.parse(file);

    //dbg!(file_node.syntax());

    let mut locals_map = HashMap::new();
    let mut checker = Checker::new();

    let main = file_node
        .syntax()
        .children()
        .filter_map(ast::Fn::cast)
        .find(|function| function.name().unwrap().text() == "main")
        .expect("no function naimed 'main' found");

    for stmt in main.body().unwrap().stmt_list().unwrap().statements() {
        info!("Processing '{}'", stmt.syntax().text());
        process_statement(
            &stmt,
            &mut checker,
            &mut locals_map,
            &semantics,
        )?;
        info!("\n{}", checker);
    }

    Ok(())
}

fn process_statement<'db, DB: HirDatabase>(
    stmt: &ast::Stmt,
    checker: &mut Checker,
    locals_map: &mut HashMap<hir::Local, VarId>,
    sema: &Semantics<'db, DB>,
) -> Result<(), CheckError> {
    match stmt {
        ast::Stmt::ExprStmt(expr) => match expr.expr().unwrap() {
            ast::Expr::BinExpr(expr) => {
                let lhs = expr.lhs().unwrap();
                let rhs = expr.rhs().unwrap();
                match &lhs {
                    ast::Expr::PathExpr(path) => {
                        let local = resolve_local_ref(path.path().unwrap(), sema).unwrap();
                        let lhs_var = *locals_map.get(&local).unwrap();
                        let rhs_var =
                            resolve_borrow_target(&rhs, checker, locals_map, sema)?;
                        checker.initialize_var_with_value(lhs_var, rhs_var)?;
                    }
                    ast::Expr::PrefixExpr(prefix_expr) => {
                        if prefix_expr.op_kind().unwrap() != ast::UnaryOp::Deref {
                            todo!();
                        }
                        match &prefix_expr.expr().unwrap() {
                            ast::Expr::PathExpr(path) => {
                                let local = resolve_local_ref(path.path().unwrap(), sema).unwrap();
                                let lhs_inner_var = *locals_map.get(&local).unwrap();
                                let lhs_var = checker.get_deref_var(lhs_inner_var);
                                let rhs_var = resolve_borrow_target(
                                    &rhs,
                                    checker,
                                    locals_map,
                                    sema,
                                )?;
                                checker.initialize_var_with_value(lhs_var, rhs_var)?;
                            }
                            _ => todo!(),
                        }
                    }
                    _ => todo!(),
                }
            }
            ast::Expr::PathExpr(expr) => {
                let local = resolve_local_ref(expr.path().unwrap(), sema).unwrap();
                let var = *locals_map.get(&local).unwrap();
                checker.check_var_usable(var)?;
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
                let new_var = checker.create_var(
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
                    let rhs = resolve_borrow_target(&init, checker, locals_map, sema)?;
                    checker.initialize_var_with_value(new_var, rhs)?;
                }
            } else {
                todo!();
            }
        }
        ast::Stmt::Item(_) => todo!(),
    }
    Ok(())
}

fn resolve_borrow_target<'db, DB: HirDatabase>(
    expr: &ast::Expr,
    checker: &mut Checker,
    locals_map: &HashMap<hir::Local, VarId>,
    sema: &Semantics<'db, DB>,
) -> Result<VarId, CheckError> {
    match expr {
        ast::Expr::Literal(literal) => {
            Ok(checker.create_literal(literal.syntax().text().to_string()))
        }
        ast::Expr::PathExpr(path) => {
            let local = resolve_local_ref(path.path().unwrap(), sema).unwrap();
            Ok(*locals_map.get(&local).unwrap())
        }
        ast::Expr::RefExpr(subexpr) => {
            let is_mut_borrow = subexpr.mut_token().is_some();

            let tmp = checker.create_ref_tmp(is_mut_borrow, subexpr.syntax().text().to_string()); // Dunno if it is correct that a tmp var created by an immutable borrow is immutable

            let target =
                resolve_borrow_target(&subexpr.expr().unwrap(), checker, locals_map, sema)?;
            checker.initialize_var_with_borrow(tmp, target, is_mut_borrow)?;

            Ok(tmp)
        }
        ast::Expr::PrefixExpr(prefix_expr) => {
            if prefix_expr.op_kind().unwrap() != ast::UnaryOp::Deref {
                todo!();
            }
            let target =
                resolve_borrow_target(&prefix_expr.expr().unwrap(), checker, locals_map, sema)?;

            Ok(checker.get_deref_var(target))
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

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error(transparent)]
    Borrowcheck(#[from] CheckerError),
}
