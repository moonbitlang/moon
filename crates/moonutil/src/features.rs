use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum FeatureGateParseError {
    #[error("Unknown feature `{0}`.\n{}", AllFeatures)]
    UnknownFeature(String),
}

/// Allowed stability value tokens (either `stable` or `unstable`)
#[allow(non_camel_case_types, unused)]
#[derive(Debug, PartialEq, Eq)]
enum Stability {
    unstable,
    stable,
}

#[allow(unused)]
impl Stability {
    fn to_bool(&self) -> bool {
        match self {
            Stability::stable => true,
            Stability::unstable => false,
        }
    }
}

macro_rules! features {
    ($(
        // represents a single feature
        // $stable should be either `stable` or `unstable`
        ($is_stable:tt, $name:ident, $desc:expr)
    ),*$(,)?) => {
        /// Represent the list of unstable features.
        /// Stringified as a comma-separated list of feature gate names.
        #[derive(Debug, Clone)]
        pub struct FeatureGate {
            $(
                #[doc = $desc]
                pub $name: bool
            ),*
        }

        impl FeatureGate {
            $(
                #[allow(non_upper_case_globals)]
                const $name: $crate::features::Stability = $crate::features::Stability::$is_stable;
            )*

            /// Print all available features and their descriptions to a writer
            pub fn print_all_features<W: std::fmt::Write>(writer: &mut W) -> std::fmt::Result {
            writeln!(writer, "Available features:")?;
            $(
                writeln!(writer, "  {} ({:?}): {}", stringify!($name), Self::$name, $desc)?;
            )*
            Ok(())
            }
        }

        impl Default for FeatureGate {
            #[allow(unused)]
            fn default() -> Self {
                Self {$(
                    $name: Self::$name.to_bool()
                ),*}
            }
        }

        impl FromStr for FeatureGate {
            type Err = FeatureGateParseError;

            #[allow(unused)]
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let mut this = Self::default();
                for val in  s.split(',') {
                    let trim  = val.trim();
                    if trim.is_empty() {
                        continue;
                    }
                    match trim {
                        $(
                            stringify!($name) => {
                                this.$name = true;
                            }
                        )*
                        _ => {
                            return Err(FeatureGateParseError::UnknownFeature(s.to_owned()));
                        }
                    }
                }
                Ok(this)
            }
        }

        impl std::fmt::Display for FeatureGate {
            #[allow(unused)]
            fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut is_first = true;
                $(
                    if Self::$name.to_bool() == false && self.$name {
                        if !is_first {
                          write!(formatter, ",")?;
                        }
                        write!(formatter, stringify!($name))?;
                        is_first = false;
                    }
                )*
                Ok(())
            }
        }
    };
}

features! {
    // This is intentionally left blank, will be populated in the future
}

impl FromStr for Box<FeatureGate> {
    type Err = FeatureGateParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Box::new(FeatureGate::from_str(s)?))
    }
}

// Plain to/from_str implementations for serde

impl serde::Serialize for FeatureGate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for FeatureGate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = FeatureGate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(
                    formatter,
                    "A string containing a comma-separated list of features"
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                FeatureGate::from_str(v).map_err(|e| serde::de::Error::custom(e))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

/// A helper type that displays all available features when printed
pub struct AllFeatures;

impl std::fmt::Display for AllFeatures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        FeatureGate::print_all_features(f)
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod test {
    use super::FeatureGateParseError;
    use std::str::FromStr;

    features! {
        (stable, test_stable, "Dummy feature that's stable"),
        (unstable, test_unstable, "Dummy feature that's unstable"),
        (unstable, test_unstable2, "Dummy feature that's unstable")
    }

    #[test]
    fn test_feature_parsing_empty() {
        let f = FeatureGate::from_str("").expect("should parse successfully");
        assert!(f.test_stable);
        assert!(!f.test_unstable);
    }

    #[test]
    fn test_feature_parsing_stable() {
        let f = FeatureGate::from_str("test_stable").expect("should parse successfully");
        assert!(f.test_stable);
        assert!(!f.test_unstable);
    }

    #[test]
    fn test_feature_parsing_unstable() {
        let f = FeatureGate::from_str("test_unstable").expect("should parse successfully");
        assert!(f.test_stable);
        assert!(f.test_unstable);
    }

    #[test]
    fn test_feature_parsing_unstable_comma() {
        let f = FeatureGate::from_str("test_unstable,test_unstable2")
            .expect("should parse successfully");
        assert!(f.test_stable);
        assert!(f.test_unstable);
        assert!(f.test_unstable2);

        let f = FeatureGate::from_str("test_unstable, test_unstable2")
            .expect("should parse successfully");
        assert!(f.test_stable);
        assert!(f.test_unstable);
        assert!(f.test_unstable2);
    }

    #[test]
    fn test_feature_parsing_unknown() {
        let result = FeatureGate::from_str("unknown_feature");
        assert!(matches!(
            result,
            Err(FeatureGateParseError::UnknownFeature(_))
        ));
    }

    #[test]
    fn test_feature_parsing_mixed_known_unknown() {
        let result = FeatureGate::from_str("test_unstable,unknown_feature");
        assert!(matches!(
            result,
            Err(FeatureGateParseError::UnknownFeature(_))
        ));
    }

    #[test]
    fn test_display_empty() {
        let f = FeatureGate::default();
        assert_eq!(f.to_string(), "");
    }

    #[test]
    fn test_display_single_unstable() {
        let f = FeatureGate {
            test_unstable: true,
            ..FeatureGate::default()
        };
        assert_eq!(f.to_string(), "test_unstable");
    }

    #[test]
    fn test_display_multiple_unstable() {
        let f = FeatureGate {
            test_unstable: true,
            test_unstable2: true,
            ..FeatureGate::default()
        };
        let display = f.to_string();
        assert_eq!(display, "test_unstable,test_unstable2");
    }

    #[test]
    fn test_display_stable_not_shown() {
        let f = FeatureGate {
            test_stable: false, // Even if disabled, stable features shouldn't show
            ..FeatureGate::default()
        };
        assert_eq!(f.to_string(), "");
    }

    #[test]
    fn test_print_all_features() {
        let mut output = String::new();
        FeatureGate::print_all_features(&mut output).expect("should write successfully");
        assert!(output.contains("Available features:"));
        assert!(output.contains("test_stable (stable)"));
        assert!(output.contains("test_unstable (unstable)"));
        assert!(output.contains("test_unstable2 (unstable)"));
        assert!(output.contains("Dummy feature that's stable"));
        assert!(output.contains("Dummy feature that's unstable"));
    }
}
