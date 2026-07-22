//! Compilation must scale near-linearly in the number of reachable top-level
//! declarations. A prior implementation captured each closure's environment by
//! deep clone, so every added declaration doubled the work of evaluating the
//! module — exponential blow-up that made even a few dozen trivial declarations
//! take minutes. These tests pin the fix: a program that is a wall of
//! independent declarations compiles in well under a second.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn compile_within(src: String, budget: Duration) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = emet::compile(&src).map(|_| ());
        let _ = tx.send(result);
    });
    match rx.recv_timeout(budget) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => panic!("compile failed: {}", e.msg),
        Err(_) => panic!("compilation did not finish within {budget:?} — inference/eval is not linear"),
    }
}

fn identity_decls(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        src.push_str(&format!("f{i} s = s\n"));
    }
    src.push_str("main = []\n");
    src
}

fn record_case_decls(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        src.push_str(&format!(
            "g{i} r =\n  case r of\n    Just x -> {{ a = x, b = x }}\n    Nothing -> {{ a = \"z\", b = \"z\" }}\n"
        ));
    }
    src.push_str("main = []\n");
    src
}

#[test]
fn many_identity_declarations_compile_fast() {
    compile_within(identity_decls(60), Duration::from_secs(5));
}

#[test]
fn many_record_case_declarations_compile_fast() {
    compile_within(record_case_decls(60), Duration::from_secs(5));
}
