use automerge::{AutoCommit, ObjType, ReadDoc, transaction::Transactable};

fn main() {
    let mut doc = AutoCommit::new();
    println!("{:?}", doc.get_all(automerge::ROOT, "messages"));
}
