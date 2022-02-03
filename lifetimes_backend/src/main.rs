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
        println!("\n\nProcessing '{}'", stmt.syntax().text());
        match stmt {
            ast::Stmt::ExprStmt(expr) => match expr.expr().unwrap() {
                ast::Expr::BinExpr(bin_expr) => {
                    let lhs = bin_expr.lhs().unwrap();
                    let rhs = bin_expr.rhs().unwrap();
                    match &lhs {
                        ast::Expr::PathExpr(path) => {
                            let local =
                                resolve_local_ref(path.path().unwrap(), &semantics).unwrap();
                            let lhs_var = *locals_map.get(&local).unwrap();
                            process_assignment_rhs(
                                &rhs,
                                lhs_var,
                                &mut vars,
                                &locals_map,
                                &semantics,
                            );
                            vars.resolve_var(lhs_var)
                                .borrow_mut()
                                .transition_initialized(&vars);
                        }
                        ast::Expr::PrefixExpr(prefix_expr) => {
                            if prefix_expr.op_kind().unwrap() != ast::UnaryOp::Deref {
                                todo!();
                            }
                            match &prefix_expr.expr().unwrap() {
                                ast::Expr::PathExpr(path) => {
                                    let local = resolve_local_ref(path.path().unwrap(), &semantics)
                                        .unwrap();
                                    let lhs_inner_var = *locals_map.get(&local).unwrap();
                                    let lhs_var = vars.get_deref_var(lhs_inner_var);
                                    
                                    process_assignment_rhs(
                                        &rhs,
                                        lhs_var,
                                        &mut vars,
                                        &locals_map,
                                        &semantics,
                                    );
                                    vars.resolve_var(lhs_var)
                                        .borrow_mut()
                                        .transition_initialized(&vars);
                                }
                                _ => todo!()
                            }
                        }
                        _ => todo!(),
                    }
                }
                _ => todo!(),
            },
            ast::Stmt::LetStmt(let_stmt) => {
                if let ast::Pat::IdentPat(ident) = let_stmt.pat().unwrap() {
                    let new_local = semantics.to_def(&ident).unwrap();
                    let new_var = vars.create_var(
                        ident.mut_token().is_some(),
                        new_local
                            .name(semantics.db)
                            .unwrap()
                            .as_text()
                            .unwrap()
                            .to_string(),
                    );
                    locals_map.insert(new_local, new_var);

                    if let Some(init) = let_stmt.initializer() {
                        process_assignment_rhs(&init, new_var, &mut vars, &locals_map, &semantics);
                        vars.resolve_var(new_var)
                            .borrow_mut()
                            .transition_initialized(&vars);
                    }
                } else {
                    todo!();
                }
            }
            ast::Stmt::Item(_) => todo!(),
        }

        println!("{}", vars);
    }
}

fn process_assignment_rhs<'db, DB: HirDatabase>(
    rhs: &ast::Expr,
    lhs: VarId,
    vars: &mut Vars,
    locals_map: &HashMap<hir::Local, VarId>,
    sema: &Semantics<'db, DB>,
) {
    match rhs {
        ast::Expr::RefExpr(expr) => {
            process_borrow_target(
                &expr.expr().unwrap(),
                lhs,
                expr.mut_token().is_some(),
                vars,
                locals_map,
                sema,
            );
        }
        ast::Expr::Literal(_) => {}
        ast::Expr::PathExpr(path) => {
            let local = resolve_local_ref(path.path().unwrap(), sema).unwrap();
            let var = *locals_map.get(&local).unwrap();
            vars.resolve_var(var).borrow_mut().transition_moved(vars);
        }
        _ => todo!(),
    }
}

fn process_borrow_target<'db, DB: HirDatabase>(
    expr: &ast::Expr,
    borrower: VarId,
    is_mut_borrow: bool,
    vars: &mut Vars,
    locals_map: &HashMap<hir::Local, VarId>,
    sema: &Semantics<'db, DB>,
) {
    match expr {
        ast::Expr::Literal(_) => {}
        ast::Expr::PathExpr(path) => {
            let local = resolve_local_ref(path.path().unwrap(), sema).unwrap();
            let mut target = vars
                .resolve_var(*locals_map.get(&local).unwrap())
                .borrow_mut();

            if is_mut_borrow {
                target.transition_mut_borrowed(borrower, vars);
            } else {
                target.transition_borrowed(borrower, vars);
            }
        }
        ast::Expr::RefExpr(subexpr) => {
            let target = vars.create_var(is_mut_borrow, format!("{}", &subexpr.syntax().text())); // Dunno it is correct that a anonymous var created by an immutable borrow is immutable
            let is_mut_subborrow = subexpr.mut_token().is_some();

            process_borrow_target(
                &subexpr.expr().unwrap(),
                target,
                is_mut_subborrow,
                vars,
                locals_map,
                sema,
            );

            let mut target = vars.resolve_var(target).borrow_mut();
            if is_mut_borrow {
                target.transition_mut_borrowed(borrower, vars);
            } else {
                target.transition_borrowed(borrower, vars);
            }
        },
        ast::Expr::PrefixExpr(prefix_expr) => {
            if prefix_expr.op_kind().unwrap() != ast::UnaryOp::Deref {
                todo!();
            }
            todo!()
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
