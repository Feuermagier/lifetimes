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
        .expect("no function named 'main' found");

    checker.enter_function(checker.static_origin());
    process_block(
        &main.body().unwrap(),
        &mut checker,
        &mut locals_map,
        &semantics,
    )?;

    Ok(())
}

fn process_block<'db, DB: HirDatabase>(
    block: &ast::BlockExpr,
    checker: &mut Checker,
    locals_map: &mut HashMap<hir::Local, VarId>,
    sema: &Semantics<'db, DB>,
) -> Result<VarId, CheckError> {
    checker.enter_scope();

    for stmt in block.stmt_list().unwrap().statements() {
        info!("Processing '{}'", stmt.syntax().text());
        process_statement(&stmt, checker, locals_map, sema)?;
        info!("\n{}", checker);
    }

    let return_var = if let Some(expr) = block.tail_expr() {
        Some(resolve_borrow_target(&expr, checker, locals_map, sema)?)
    } else {
        None
    };

    checker.leave_scope(return_var)?;

    Ok(return_var.unwrap_or_else(|| checker.void_literal()))
}

fn process_statement<'db, DB: HirDatabase>(
    stmt: &ast::Stmt,
    checker: &mut Checker,
    locals_map: &mut HashMap<hir::Local, VarId>,
    sema: &Semantics<'db, DB>,
) -> Result<(), CheckError> {
    match stmt {
        ast::Stmt::ExprStmt(expr) => {
            let expr = expr.expr().unwrap();
            let _ = resolve_borrow_target(&expr, checker, locals_map, sema)?;
        }
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
                    checker.initialize_var_with_value(new_var, vec![rhs])?;
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
    locals_map: &mut HashMap<hir::Local, VarId>,
    sema: &Semantics<'db, DB>,
) -> Result<VarId, CheckError> {
    match expr {
        ast::Expr::Literal(literal) => {
            Ok(checker.create_literal(literal.syntax().text().to_string()))
        }
        ast::Expr::PathExpr(path) => {
            let local = resolve_local_ref(path.path().unwrap(), sema).unwrap();
            let var = *locals_map.get(&local).unwrap();
            checker.check_var_usable(var)?;
            Ok(var)
        }
        ast::Expr::RefExpr(subexpr) => {
            let is_mut_borrow = subexpr.mut_token().is_some();

            let tmp = checker.create_ref_tmp(is_mut_borrow, subexpr.syntax().text().to_string()); // Dunno if it is correct that a tmp var created by an immutable borrow is immutable

            let target =
                resolve_borrow_target(&subexpr.expr().unwrap(), checker, locals_map, sema)?;
            checker.initialize_var_with_borrow(tmp, vec![target], is_mut_borrow)?;

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
        ast::Expr::BinExpr(bin_expr) => {
            let lhs = resolve_borrow_target(&bin_expr.lhs().unwrap(), checker, locals_map, sema)?;
            let rhs = resolve_borrow_target(&bin_expr.rhs().unwrap(), checker, locals_map, sema)?;
            match bin_expr.op_kind().unwrap() {
                ast::BinaryOp::LogicOp(_) => todo!(),
                ast::BinaryOp::ArithOp(_) => todo!(),
                ast::BinaryOp::CmpOp(_) => todo!(),
                ast::BinaryOp::Assignment { op } => {
                    checker.initialize_var_with_value(lhs, vec![rhs])?;
                    // An assignment returns a new var of type void
                    Ok(checker.void_literal())
                }
            }
        }
        ast::Expr::IfExpr(expr) => {
            let cond = expr.condition().unwrap().expr().unwrap();
            let _ = resolve_borrow_target(&cond, checker, locals_map, sema)?;

            let mut vars = vec![process_block(
                &expr.then_branch().unwrap(),
                checker,
                locals_map,
                sema,
            )?];
            let mut else_branch = expr.else_branch();
            while let Some(branch) = &else_branch {
                match branch {
                    ast::ElseBranch::Block(block) => {
                        vars.push(process_block(&block, checker, locals_map, sema)?);
                        break;
                    }
                    ast::ElseBranch::IfExpr(expr) => {
                        vars.push(process_block(
                            &expr.then_branch().unwrap(),
                            checker,
                            locals_map,
                            sema,
                        )?);
                        else_branch = expr.else_branch();
                    }
                }
            }
            let expr_value_var = checker.create_var(false, false, "<if rslt>".to_string());
            checker.initialize_var_with_value(expr_value_var, vars)?;

            Ok(expr_value_var)
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
