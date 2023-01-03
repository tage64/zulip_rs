const SEARCH_OPERAND: &str = "search";

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
}
