use std::sync::Arc;

use base_db::{CrateOrigin, Env, FilePosition};
use hir::{CfgOptions, Semantics};
use ide::{
    AnalysisHost, AssistResolveStrategy, Change, CrateGraph, DiagnosticsConfig, Edition, FileId,
    SourceRoot, TextSize,
};
use syntax::{
    ast::{self, AstNode},
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
    let y = &mut x;
    *y = 3;
    x = 4;
}"#
            .to_string(),
        )),
    );

    host.apply_change(initial_change);

    let analysis = host.analysis();

    let offset = analysis
        .file_line_index(file)
        .unwrap()
        .offset(ide::LineCol { line: 3, col: 17 });

    let semantics = Semantics::new(host.raw_database());
    let file_node = semantics.parse(file);
    let ast = file_node.syntax();
    //semantics.descend_into_macros()

    let token = ast.token_at_offset(offset);
    let token = match token {
        syntax::TokenAtOffset::None => todo!(),
        syntax::TokenAtOffset::Single(_) => todo!(),
        syntax::TokenAtOffset::Between(lhs, rhs) => rhs,
    };

    let function = token.ancestors().find_map(ast::Fn::cast).unwrap();
    let stmt = token.ancestors().find_map(ast::Stmt::cast).unwrap();
    semantics.hir_file_for(stmt.syntax());
    
    
    let expressions = stmt
        .syntax()
        .siblings_with_tokens(Direction::Next)
        .filter_map(NodeOrToken::into_node)
        .filter_map(ast::Stmt::cast)
        .collect::<Vec<_>>();
    dbg!(expressions
        .iter()
        .map(|expr| expr.syntax().text().to_string())
        .collect::<Vec<String>>());
}
