use anyhow::Context;
use reshiki::chemistry::cdxml::{self, ObjectMapEntry};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};
#[derive(Deserialize)]
struct Mapping {
    source: usize,
    atoms: Vec<u64>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    objects: Vec<Mapping>,
    first: u64,
    expected: Option<Value>,
    failure: Option<String>,
    restriction: Option<String>,
}
#[test]
fn logical_groups_match_original_reader() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/cdxml_groups_reference.py"))
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let (mut accepted, mut rejected, mut restricted) = (0, 0, 0);
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let objects = case
            .objects
            .into_iter()
            .map(|m| ObjectMapEntry {
                source: m.source,
                atoms: m.atoms,
            })
            .collect::<Vec<_>>();
        let before = serde_json::to_value((&objects, &case.text))?;
        let result = cdxml::read_groups(&case.text, &objects, case.first);
        assert_eq!(before, serde_json::to_value((&objects, &case.text))?);
        if case.restriction.is_some() {
            restricted += 1;
            anyhow::ensure!(
                case.expected.is_some() && result.is_err(),
                "{}: bad restriction",
                case.name
            );
        } else {
            match (result, case.expected) {
                (Ok(result), Some(expected)) => {
                    accepted += 1;
                    assert_eq!(serde_json::to_value(result)?, expected, "{}", case.name);
                }
                (Err(_), None) => rejected += 1,
                (result, expected) => anyhow::bail!(
                    "{}: {result:?} != {expected:?}; {:?}",
                    case.name,
                    case.failure
                ),
            }
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CDXML groups: {accepted} exact helpers, {rejected} original failures, {restricted} separate restrictions"
    );
    assert!(accepted > 600 && rejected == 1 && restricted == 4);
    Ok(())
}
#[test]
fn twenty_thousand_groups_preserve_reverse_source_order() -> anyhow::Result<()> {
    let mut text = String::from("<CDXML>");
    let mut objects = Vec::new();
    for i in 0..20_000usize {
        text.push_str("<group><n/><n/></group>");
        objects.push(ObjectMapEntry {
            source: i * 3 + 2,
            atoms: vec![u64::try_from(i)? * 2 + 1],
        });
        objects.push(ObjectMapEntry {
            source: i * 3 + 3,
            atoms: vec![u64::try_from(i)? * 2 + 2],
        });
    }
    text.push_str("</CDXML>");
    let groups = cdxml::read_groups(&text, &objects, 40_001)?;
    assert_eq!(groups.len(), 20_000);
    for (i, group) in groups.iter().enumerate() {
        assert_eq!(group.id, 40_001 + u64::try_from(i)?);
        assert_eq!(
            group.members,
            vec![
                39_999 - u64::try_from(i)? * 2,
                40_000 - u64::try_from(i)? * 2
            ]
        );
    }
    Ok(())
}
#[test]
fn group_limits_fail_without_mutation() -> anyhow::Result<()> {
    let objects = vec![
        ObjectMapEntry {
            source: 0,
            atoms: vec![]
        };
        100_001
    ];
    assert!(matches!(
        cdxml::read_groups("<CDXML/>", &objects, 1),
        Err(cdxml::Error::Limit)
    ));
    let objects = [ObjectMapEntry {
        source: 0,
        atoms: vec![1; 1_000_001],
    }];
    assert!(matches!(
        cdxml::read_groups("<CDXML/>", &objects, 1),
        Err(cdxml::Error::Limit)
    ));
    assert!(
        cdxml::read_groups(
            "<CDXML/>",
            &[ObjectMapEntry {
                source: usize::MAX,
                atoms: vec![]
            }],
            1
        )
        .is_err()
    );
    Ok(())
}
