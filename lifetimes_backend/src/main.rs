use std::{collections::HashMap, path::Path, sync::Arc};

use base_db::{CrateOrigin, Env, FilePosition};
use hir::{db::HirDatabase, CfgOptions, Semantics};
use ide::{
    AnalysisHost, AssistResolveStrategy, Change, CrateGraph, DiagnosticsConfig, Edition, FileId,
    SourceRoot, TextSize,
};
use syntax::{
    ast::{self, AstNode, TokenTree},
    AstToken, Direction, NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, T,
};
use tracing_subscriber::filter::LevelFilter;
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
    let mut x = 5;
    let y = &x;
    let z = &mut x;
    //*y = 3;
    //x = 4;
}"#
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

    //dbg!(ast);

    let mut vars = HashMap::new();

    let local = semantics
        .to_def(&token.ancestors().find_map(ast::IdentPat::cast).unwrap())
        .unwrap();

    vars.insert(
        local,
        Var {
            status: VarStatus::Available,
            borrows: vec![],
        },
    );

    let parent_stmt = token.ancestors().find_map(ast::Stmt::cast).unwrap();

    for stmt in parent_stmt
        .syntax()
        .siblings(Direction::Next)
        .filter_map(ast::Stmt::cast)
    {
        match stmt {
            ast::Stmt::ExprStmt(expr) => match expr.expr().unwrap() {
                ast::Expr::BinExpr(bin_expr) => {
                    let lhs = bin_expr.lhs().unwrap();
                    let rhs = bin_expr.rhs().unwrap();
                }
                _ => todo!(),
            },
            ast::Stmt::LetStmt(let_stmt) => {
                if let ast::Pat::IdentPat(ident) = let_stmt.pat().unwrap() {
                    let new_local = semantics.to_def(&ident).unwrap();
                    let mut new_var = Var { status: VarStatus::Available, borrows: vec![] };
                    let init = let_stmt.initializer().unwrap();
                    process_rhs(&init, &mut new_var, &mut vars, &semantics);
                    vars.insert(new_local, new_var);
                } else {
                    todo!();
                }
            }
            ast::Stmt::Item(_) => todo!(),
        }
    }

    for (local, var) in vars {
        let name = local.name(semantics.db).unwrap().as_text().unwrap();
        println!("Local '{}' status: {:?}", name, var.status);
    }
}

fn process_rhs<'a, 'db, DB: HirDatabase>(
    rhs: &ast::Expr,
    target: &mut Var,
    vars: &mut HashMap<hir::Local, Var>,
    sema: &Semantics<'db, DB>,
) {
    match rhs {
        ast::Expr::RefExpr(expr) => {
            let is_mut_ref = expr.mut_token().is_some();
            let local = match expr.expr().unwrap() {
                ast::Expr::PathExpr(path) => resolve_local_ref(path.path().unwrap(), sema).unwrap(),
                _ => todo!(),
            };
            let var = vars.get_mut(&local).unwrap();
            match &mut var.status {
                VarStatus::Available => var.status = if is_mut_ref { VarStatus::MutBorrowed } else { VarStatus::Borrowed(1) },
                VarStatus::Borrowed(i) => if !is_mut_ref { *i += 1 } else { panic!("Mutably borrowing a var that is already borrowed")} , 
                VarStatus::MutBorrowed => panic!("Borrowing a mutably borrowed var"),
                VarStatus::Moved => panic!("Borrowing a moved var"),
            }
            target.borrows.push(local);
        }
        _ => println!("Skipping {}", rhs.syntax().text()),
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

#[derive(Debug)]
struct Var {
    status: VarStatus,
    borrows: Vec<hir::Local>,
}

#[derive(Debug)]
enum VarStatus {
    Available,
    Borrowed(u32),
    MutBorrowed,
    Moved,
}
