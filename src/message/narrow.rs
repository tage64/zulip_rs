use anyhow::Context as _;
use pest::Parser;

const SEARCH_OPERAND: &str = "search";

#[derive(pest_derive::Parser)]
#[grammar = "message/narrow.pest"]
struct NarrowParser;

/// A filter for Zulip messages.
///
/// A narrow is a set of filters for Zulip messages, that can be based on many different factors
/// (like sender, stream, topic, search keywords, etc.). Narrows are used in various places in the
/// the Zulip API (most importantly, in the API for fetching messages).
///
/// Read more about narrows [here](https://zulip.com/api/construct-narrow).
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
pub struct Narrow {
    pub operand: String,
    pub operator: String,
    pub negated: bool,
}

impl Narrow {
    /// Create a narrow from a search keyword.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zulip::message::Narrow;
    ///
    /// let narrow = Narrow::keyword("discrimination tree".to_string());
    /// assert_eq!(
    ///     narrow,
    ///     Narrow { operand: "search".to_string(), operator: "discrimination tree".to_string(), negated: false },
    /// );
    /// ```
    pub fn keyword(keyword: String) -> Self {
        Self {
            operand: SEARCH_OPERAND.to_string(),
            operator: keyword,
            negated: false,
        }
    }

    /// Parse a filter on the form "[-]<FILTERNAME>:<VALUE>" or a keyword otherwise.
    ///
    /// The function will basicly parse a filter if there is a colon (':') in the string (negating
    /// it if it starts with a dash ('-')), and if the string doesn't contains a colon it will be
    /// interpretted as a keyword search.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zulip::message::Narrow;
    ///
    /// let q = "stream:lean4";
    /// assert_eq!(
    ///     Narrow::parse(q),
    ///     Narrow { operand: "stream".to_string(), operator: "lean4".to_string(), negated: false },
    /// );
    /// let q = "-is:read";
    /// assert_eq!(
    ///     Narrow::parse(q),
    ///     Narrow { operand: "is".to_string(), operator: "read".to_string(), negated: true },
    /// );
    /// let q = "keyword";
    /// assert_eq!(
    ///     Narrow::parse(q),
    ///     Narrow { operand: "search".to_string(), operator: "keyword".to_string(), negated: false },
    /// );
    /// ```
    pub fn parse(text: &str) -> Self {
        match text.split_once(':') {
            None => Self::keyword(text.to_string()),
            Some((operand, operator)) => {
                let (negated, operand) = if let Some(tail) = operand.strip_prefix('-') {
                    (true, tail)
                } else {
                    (false, operand)
                };
                Self {
                    operand: operand.to_string(),
                    operator: operator.to_string(),
                    negated,
                }
            }
        }
    }

    /// Parse a search query from a human read/writable string.
    ///
    /// The syntax can be explained by the following BNF grammar:
    /// ```BNF
    /// <SEARCH_QUERY> ::= <FILTER>* <KEYWORD>*
    /// <KEYWORD> ::= <STRING>
    /// <FILTER> ::= <OPERAND>:[<NEGATION>]<OPERATOR>
    /// <OPERAND> ::= <STRING>
    /// <OPERATOR> ::= <STRING>
    /// <NEGATION> ::= "-"
    /// ```
    /// Whitespaces might be inserted anywhere. A `<STRING>` is either enclosed in quotes ('"') or
    /// not. In both cases  it supports the escape sequences from the [snailquote
    /// crate](https://docs.rs/snailquote/latest/snailquote/fn.unescape.html).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zulip::message::Narrow;
    ///
    /// let q =r#"stream:lean4 topic:"discrimination tree lookup" -is:unread example "\" escaped" "#;
    /// let json =r#"
    ///     [
    ///         { "operand": "stream", "operator": "lean4", "negated": false },
    ///         { "operand": "topic", "operator": "discrimination tree lookup", "negated": false },
    ///         { "operand": "is", "operator": "unread", "negated": true },
    ///         { "operand": "search", "operator": "example", "negated": false },
    ///         { "operand": "search", "operator": "\" escaped", "negated": false }
    ///     ]"#;
    /// assert_eq!(
    ///     serde_json::to_value(Narrow::parse_(q).unwrap()).unwrap(),
    ///     serde_json::from_str::<'_, serde_json::Value>(json).unwrap()
    /// );
    /// ```
    pub fn parse_(text: &str) -> anyhow::Result<Vec<Self>> {
        let parsed = NarrowParser::parse(Rule::NARROW, text)
            .with_context(|| format!("Failed to parse narrow: {text}"))?
            .next()
            .context("Cannot parse narrow from: {text:?}")?;
        let (parsed_filters, parsed_keywords) = {
            let mut pairs = parsed.into_inner();
            (pairs.next().unwrap(), pairs.next().unwrap())
        };
        debug_assert_eq!(parsed_filters.as_rule(), Rule::FILTERS);
        debug_assert_eq!(parsed_keywords.as_rule(), Rule::KEYWORDS);

        // Get the string content from a pest-pair of `Rule::STRING`.
        let get_string = |parsed_string: pest::iterators::Pair<'_, _>| {
            debug_assert_eq!(parsed_string.as_rule(), Rule::STRING);
            snailquote::unescape(parsed_string.as_str())
        };

        let filters = parsed_filters.into_inner().map(|parsed_filter| {
            debug_assert_eq!(parsed_filter.as_rule(), Rule::FILTER);
            let mut pairs = parsed_filter.into_inner();
            let next_pair = pairs.next().unwrap();
            let (negated, operand) = match next_pair.as_rule() {
                Rule::NEGATION => (true, get_string(pairs.next().unwrap())?),
                Rule::STRING => (false, get_string(next_pair)?),
                _ => unreachable!(),
            };
            let operator = get_string(pairs.next().unwrap())?;
            Ok(Self {
                operand,
                operator,
                negated,
            })
        });

        let keywords = parsed_keywords.into_inner().map(|parsed_keyword| {
            Ok(Self {
                operand: SEARCH_OPERAND.to_owned(),
                operator: get_string(parsed_keyword)?,
                negated: false,
            })
        });

        filters.chain(keywords).collect()
    }
}
