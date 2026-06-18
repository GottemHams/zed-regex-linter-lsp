use super::LinterConfig;

pub const SOURCE: &str = "annotations";

pub fn config() -> LinterConfig {
	return LinterConfig {
		enabled: Some(false),
		comments_only: Some(true),
		on_save: Some(false),
		languages: None,
		error: Some(["FIXME", "ERROR"].map(String::from).to_vec()),
		warning: Some(["TODO", "@todo", "WIP", "WARNING"].map(String::from).to_vec()),
		info: Some(["NOTE", "README", "INFO"].map(String::from).to_vec()),
	};
}
