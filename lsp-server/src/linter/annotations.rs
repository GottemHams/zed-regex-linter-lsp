use super::LinterConfig;

pub const SOURCE: &str = "annotations";

pub fn config() -> LinterConfig {
	return LinterConfig {
		enabled: Some(false),
		comments_only: Some(true),
		languages: None,
		error: Some(["\\bFIXME\\b", "\\bERROR\\b"].map(String::from).to_vec()),
		warning: Some(["\\bTODO\\b", "@todo\\b", "\\bWIP\\b", "\\bWARNING\\b"].map(String::from).to_vec()),
		info: Some(["\\bNOTE\\b", "\\bREADME\\b", "\\bINFO\\b"].map(String::from).to_vec()),
	};
}
