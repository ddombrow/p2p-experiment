#[tokio::main]
async fn main() {
    let mut doc = p2p_experiment::doc::Doc::new();
    doc.add_objective("Test", "unassigned");
    let board = doc.read();
    println!("{:?}", board.objectives);
}
