use std::sync::Arc;

use base_db::{Env, CrateOrigin, FilePosition};
use hir::{CfgOptions, Semantics};
use ide::{AnalysisHost, Change, FileId, CrateGraph, SourceRoot, DiagnosticsConfig, AssistResolveStrategy, Edition};
use vfs::{file_set::FileSet, VfsPath};
use syntax::ast::AstNode;

fn main() {
    let mut host = AnalysisHost::new(None);

    let mut file_set = FileSet::default();
    let file = FileId(0);
    file_set.insert(file, VfsPath::new_virtual_path("/main.rs".to_string()));
    let root = SourceRoot::new_local(file_set);

    let mut crate_graph = CrateGraph::default();
    crate_graph.add_crate_root(file, Edition::Edition2021, None, None, CfgOptions::default(), CfgOptions::default(), Env::default(), Vec::new(), CrateOrigin::Unknown);


    let mut initial_change = Change::default();
    initial_change.set_roots(vec![root]);
    initial_change.set_crate_graph(crate_graph);
    initial_change.change_file(FileId(0), Some(Arc::new(r#"fn main() {
        let mut x = 5;
        let y = &mut x;
        *y = 3;
        x = 4;
    }"#.to_string())));

    host.apply_change(initial_change);

    let analysis = host.analysis();
    println!("{:?}", analysis.file_text(file));
    println!("{:?}", analysis.view_hir(FilePosition {file_id: file, offset: 0.into()}));
    println!("{:?}", analysis.diagnostics(&DiagnosticsConfig::default(), AssistResolveStrategy::All, file));

    let semantics = Semantics::new(host.raw_database());
    let file_node = semantics.parse(file);
    let ast = file_node.syntax();
    semantics.descend_into_macros();
    dbg!(ast);
}
