//! #1042: import keeps the E/Z under a wU/wD flag where the flag cannot mean a
//! wedge. RDKit drops it, and the worker reads E/Z from its own depiction.
//! Compare acceptance for these inputs; native_import tests check the kept E/Z,
//! and smiles_read compares RDKit's reading for every input.
use reshiki::chemistry::smiles;

/// Inputs whose whole prepared molecule changes when the stated E/Z is kept.
pub fn flagged(format: &str, text: &str) -> bool {
    if format != "smiles" {
        return false;
    }
    let (Ok(kept), Ok(dropped)) = (smiles::read(text), smiles::read_dropping_flagged_ez(text))
    else {
        return false;
    };
    serde_json::to_value(kept.prepared).expect("Prepared molecule serialization")
        != serde_json::to_value(dropped.prepared).expect("Prepared molecule serialization")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_prepared_state_selects_the_response_exception() {
        for text in [
            "C/C=C/C |wU:0.0|",
            r"C/C=C\C |wD:1.0|",
            "[H]/N=C/F |wU:1.0|",
            "C/C=C/C |(0,1,;0,0,;1,0,;1,-1,),wU:1.0|",
        ] {
            assert!(flagged("smiles", text), "{text}");
        }
    }

    #[test]
    fn unchanged_prepared_state_keeps_the_response_comparison() {
        for text in [
            "C/C=C/C",
            "C/C=C |wU:1.0|",
            "C/C=C/C |wU:1.0,ctu:1|",
            "[H]/CC=C/C |wU:3.3|",
            "F/C=C/F |wU:1.0|",
            "C/C(F)=C/F |wU:1.0|",
            "C/C=C=CC |wU:1.0|",
        ] {
            smiles::read(text).unwrap();
            smiles::read_dropping_flagged_ez(text).unwrap();
            assert!(!flagged("smiles", text), "{text}");
        }
    }

    #[test]
    fn other_formats_and_invalid_inputs_keep_the_response_comparison() {
        for format in ["mol", "rxn", "rsmi"] {
            assert!(!flagged(format, "C/C=C/C |wU:0.0|"), "{format}");
        }
        assert!(!flagged("smiles", "C1CC |wU:0.0|"));
    }
}
