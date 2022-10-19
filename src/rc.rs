use anyhow::Result;
use pest_derive::*;
use serde::Deserialize;

use pest::Parser;

#[derive(Parser)]
#[grammar = "rc.pest"]
pub struct INIParser;

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ZulipRuntimeConfig {
    pub api: ZulipAPISettings,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ZulipAPISettings {
    pub email: String,
    pub key: String,
    pub site: String,
}

pub fn parse_from_str(rc: &str) -> Result<ZulipRuntimeConfig> {
    let pairs = INIParser::parse(Rule::file, rc)?;
    let mut email = "";
    let mut key = "";
    let mut site = "";
    for pair in pairs {
        // A pair is a combination of the rule which matched and a span of input
        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::section => {
                    if inner_pair.as_str() != "[api]" {
                        panic!("not valid section")
                    }
                }
                Rule::property => {
                    let mut rule = inner_pair.into_inner();
                    let name: &str = rule.next().unwrap().as_str();
                    if name == "email" {
                        email = rule.next().unwrap().as_str();
                    }
                    if name == "key" {
                        key = rule.next().unwrap().as_str();
                    }
                    if name == "site" {
                        site = rule.next().unwrap().as_str();
                    }
                }
                Rule::EOI => break,
                _ => println!("{:?}", inner_pair),
            };
        }
    }
    Ok(ZulipRuntimeConfig {
        api: ZulipAPISettings {
            email: email.to_string(),
            key: key.to_string(),
            site: site.to_string(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_from_str() {
        let email = "me@example.com".to_string();
        let key = "1aBC9afGhIjKLmNoPqR45Stuv09WvXyZ".to_string();
        let site = "https://leanprover.zulipchat.com".to_string();
        assert_eq!(
            parse_from_str(
                indoc::formatdoc! {
                    "[api]
                    email={email}
                    key={key}
                    site={site}
                "
                }
                .as_str()
            )
            .unwrap(),
            ZulipRuntimeConfig {
                api: ZulipAPISettings { email, key, site }
            }
        );
    }
}
