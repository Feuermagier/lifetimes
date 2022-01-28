use std::sync::Arc;

use base_db::{Env, CrateOrigin};
use hir::CfgOptions;
use ide::{AnalysisHost, Change, FileId, CrateGraph, SourceRoot, DiagnosticsConfig, AssistResolveStrategy, Edition};
use vfs::{file_set::FileSet, VfsPath};

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
    initial_change.change_file(FileId(0), Some(Arc::new("fn main() {}".to_string())));

    host.apply_change(initial_change);

    let analysis = host.analysis();
    println!("{:?}", analysis.file_text(FileId(0)));
    println!("{:?}", analysis.diagnostics(&DiagnosticsConfig::default(), AssistResolveStrategy::All, file));
}
