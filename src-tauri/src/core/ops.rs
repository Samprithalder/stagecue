//! Pure cue-list edit operations (order, duplicates, grouping).
//! Unit-testable without any runtime.

use crate::core::model::Project;

/// Move a cue inside the list; returns Err when indexes are out of range.
pub fn move_cue(project: &mut Project, from: usize, to: usize) -> Result<(), String> {
    if from >= project.cues.len() || to >= project.cues.len() || from == to {
        return Err("invalid cue move indexes".into());
    }
    let cue = project.cues.remove(from);
    project.cues.insert(to, cue);
    Ok(())
}

/// Duplicate a cue right after its original with a fresh id and "copy" suffix.
pub fn duplicate_cue(project: &Project, cue_id: &str) -> Vec<crate::core::model::Cue> {
    let Some(original) = project.cue(cue_id) else {
        return project.cues.clone();
    };
    let mut copy = original.clone();
    copy.id = uuid::Uuid::new_v4().to_string();
    copy.name = format!("{} (copy)", original.name);
    copy.notes = original.notes.clone();
    let mut out = Vec::with_capacity(project.cues.len() + 1);
    let mut inserted = false;
    for cue in &project.cues {
        out.push(cue.clone());
        if cue.id == cue_id {
            out.push(copy.clone());
            inserted = true;
        }
    }
    if !inserted {
        out.push(copy);
    }
    out
}

/// Renumber cues sequentially (Q1..Qn) preserving explicit user numbers where
/// they are unique; used after bulk imports and drag reorder.
pub fn renumber(project: &mut Project) {
    for (idx, cue) in project.cues.iter_mut().enumerate() {
        if cue.number.is_empty() {
            cue.number = format!("Q{}", idx + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Cue;

    fn project_with_3() -> Project {
        let mut p = Project::new("t");
        p.cues = vec![Cue::new("Q1", "A"), Cue::new("Q2", "B"), Cue::new("Q3", "C")];
        p
    }

    #[test]
    fn move_cue_reorders() {
        let mut p = project_with_3();
        move_cue(&mut p, 0, 2).unwrap();
        let names: Vec<&str> = p.cues.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["B", "C", "A"]);
        move_cue(&mut p, 2, 0).unwrap();
        let names: Vec<&str> = p.cues.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["A", "B", "C"]);
    }

    #[test]
    fn move_cue_out_of_range_fails() {
        let mut p = project_with_3();
        assert!(move_cue(&mut p, 0, 9).is_err());
        assert!(move_cue(&mut p, 9, 0).is_err());
    }

    #[test]
    fn duplicate_inserts_copy_after_original() {
        let p = project_with_3();
        let out = duplicate_cue(&p, &p.cues[1].id);
        assert_eq!(out.len(), 4);
        assert_eq!(out[2].name, "B (copy)", "names={:?}", out.iter().map(|c| c.name.clone()).collect::<Vec<_>>());
        assert_eq!(out[3].name, "C");
        let original_id = p.cues[1].id.clone();
        assert_ne!(out[2].id, original_id);
    }
}
