//! Frozen intermediate tree transformations from the pinned OPSIN source.
//! This gate starts from supplied upstream trees, independently of native
//! frontend behavior. Test execution and fixture decompression are Rust only.
use flate2::read::GzDecoder;
use opsin::ParseOptions;
use opsin::component_generator::{ComponentGenerationContext, process_components};
use opsin::parse_tree::{Arena, NodeId, ParseTree};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};

// The upstream Element model permits duplicate ordered attributes, and its
// toXML output deliberately preserves them. A standards-based XML DOM rejects
// those trees, so read the source serializer's format without collapsing attrs.
fn tree_from_xml(xml: &str) -> ParseTree {
    struct Reader<'a> {
        input: &'a str,
        position: usize,
        arena: Arena,
    }
    fn decode(value: &str) -> String {
        let mut output = String::new();
        let mut rest = value;
        while let Some(start) = rest.find('&') {
            output.push_str(&rest[..start]);
            let end = rest[start..].find(';').unwrap() + start;
            let entity = &rest[start + 1..end];
            match entity {
                "amp" => output.push('&'),
                "lt" => output.push('<'),
                "gt" => output.push('>'),
                "quot" => output.push('"'),
                "apos" => output.push('\''),
                _ => {
                    let number = if let Some(hex) = entity.strip_prefix("#x") {
                        u32::from_str_radix(hex, 16).unwrap()
                    } else {
                        entity.strip_prefix('#').unwrap().parse().unwrap()
                    };
                    output.push(char::from_u32(number).unwrap());
                }
            }
            rest = &rest[end + 1..];
        }
        output.push_str(rest);
        output
    }
    impl Reader<'_> {
        fn take(&mut self, expected: &str) {
            assert!(self.input[self.position..].starts_with(expected));
            self.position += expected.len();
        }
        fn whitespace(&mut self) {
            while self
                .input
                .as_bytes()
                .get(self.position)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.position += 1;
            }
        }
        fn until(&mut self, character: char) -> &str {
            let start = self.position;
            self.position += self.input[start..].find(character).unwrap();
            &self.input[start..self.position]
        }
        fn element(&mut self) -> NodeId {
            self.take("<");
            let start = self.position;
            while !matches!(self.input.as_bytes()[self.position], b' ' | b'>') {
                self.position += 1;
            }
            let name = &self.input[start..self.position];
            let grouping = [
                "molecule",
                "wordRule",
                "word",
                "root",
                "substituent",
                "bracket",
                "functionalTerm",
            ]
            .contains(&name);
            let id = if grouping {
                self.arena.grouping(name)
            } else {
                self.arena.token(name, "")
            };
            loop {
                self.whitespace();
                if self.input[self.position..].starts_with('>') {
                    break;
                }
                let key = self.until('=').to_owned();
                self.take("=\"");
                let value = decode(self.until('"'));
                self.take("\"");
                self.arena[id].add_attribute(key, value);
            }
            self.take(">");
            if grouping {
                self.whitespace();
                while !self.input[self.position..].starts_with("</") {
                    let child = self.element();
                    self.arena.add_child(id, child);
                    self.whitespace();
                }
            } else {
                let value = decode(self.until('<'));
                self.arena[id].set_value(value);
            }
            self.take("</");
            self.take(name);
            self.take(">");
            id
        }
    }
    let mut reader = Reader {
        input: xml,
        position: 0,
        arena: Arena::default(),
    };
    let root = reader.element();
    assert_eq!(reader.position, xml.len());
    let tree = ParseTree {
        arena: reader.arena,
        root,
    };
    assert_eq!(tree.to_xml(), xml, "fixture tree round trip");
    tree
}

#[test]
fn component_generation_matches_all_frozen_ranked_candidates() {
    let fixtures = GzDecoder::new(include_bytes!("fixtures/component-golden.jsonl.gz").as_slice());
    let mut rows = 0;
    let mut candidates = 0;
    let mut failures = Vec::new();
    let mut failure_count = 0;
    for line in BufReader::new(fixtures).lines() {
        let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let input = row["input"].as_str().unwrap();
        rows += 1;
        let options = ParseOptions {
            allow_radicals: row["allow_radicals"].as_bool().unwrap(),
            detailed_failure_analysis: row["detailed_failure_analysis"].as_bool().unwrap(),
            ..Default::default()
        };
        for (index, expected) in row["parses"].as_array().into_iter().flatten().enumerate() {
            candidates += 1;
            let mut tree = tree_from_xml(expected["before"].as_str().unwrap());
            let mut context = ComponentGenerationContext {
                options,
                warnings: Vec::new(),
            };
            let mut actual = expected.clone();
            match process_components(&mut tree, &mut context) {
                Ok(()) => {
                    actual["after"] = json!(tree.to_xml());
                    actual["error"] = Value::Null
                }
                Err(error) => {
                    actual["after"] = Value::Null;
                    actual["error"] = json!(error.to_string())
                }
            }
            actual["warnings"] = json!(
                context
                    .warnings
                    .iter()
                    .map(|w| json!({"kind":w.kind.as_str(),"message":w.message}))
                    .collect::<Vec<_>>()
            );
            if actual != *expected {
                failure_count += 1;
                if failures.len() < 12 {
                    let mut diff = format!("{input:?}, ranked candidate {index}");
                    for field in ["error", "warnings", "after"] {
                        if actual[field] == expected[field] {
                            continue;
                        }
                        if field == "after"
                            && let (Some(a), Some(e)) =
                                (actual[field].as_str(), expected[field].as_str())
                        {
                            let a = a.lines().collect::<Vec<_>>();
                            let e = e.lines().collect::<Vec<_>>();
                            let line = a
                                .iter()
                                .zip(&e)
                                .position(|(a, e)| a != e)
                                .unwrap_or(a.len().min(e.len()));
                            diff.push_str(&format!(
                                "\nafter first differing line {line}: expected {:?}, actual {:?}",
                                e.get(line),
                                a.get(line)
                            ));
                            continue;
                        }
                        diff.push_str(&format!(
                            "\n{field}: expected {}, actual {}",
                            expected[field], actual[field]
                        ));
                    }
                    failures.push(diff);
                }
            }
        }
    }
    assert_eq!(rows, 4320);
    assert_eq!(candidates, 4736);
    assert_eq!(
        failure_count,
        0,
        "ComponentGenerator differences ({failure_count}/{candidates}):\n{}",
        failures.join("\n\n")
    );
}
