//! Topological sort of claims by their `after` edges.
//!
//! Used to order the apply pass in reconcile. Orphan removal runs in
//! reverse topological order (you stop the service before you remove
//! the file before you purge the package).

use anyhow::{bail, Result};
use golem_types::{Claim, ClaimId};
use std::collections::{HashMap, HashSet};

pub fn topo_order(claims: &[Claim]) -> Result<Vec<ClaimId>> {
    let by_id: HashMap<&ClaimId, &Claim> =
        claims.iter().map(|c| (&c.id, c)).collect();

    let mut visited: HashSet<ClaimId> = HashSet::new();
    let mut stack: HashSet<ClaimId>   = HashSet::new();
    let mut out: Vec<ClaimId>         = Vec::with_capacity(claims.len());

    fn visit(
        id: &ClaimId,
        by_id: &HashMap<&ClaimId, &Claim>,
        visited: &mut HashSet<ClaimId>,
        stack:   &mut HashSet<ClaimId>,
        out:     &mut Vec<ClaimId>,
    ) -> Result<()> {
        if visited.contains(id) { return Ok(()); }
        if !stack.insert(id.clone()) { bail!("dependency cycle at {id}"); }
        if let Some(c) = by_id.get(id) {
            for dep in &c.after {
                visit(dep, by_id, visited, stack, out)?;
            }
        }
        stack.remove(id);
        visited.insert(id.clone());
        out.push(id.clone());
        Ok(())
    }

    for c in claims {
        visit(&c.id, &by_id, &mut visited, &mut stack, &mut out)?;
    }
    Ok(out)
}
