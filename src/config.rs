use std::any::type_name;

use config::Format;
use itertools::Itertools;
use kameo::Reply;
use serde::{Deserialize, Serialize};

// Config is another place a provider pattern AKA associative product types, would be good
// You could just pass Config as database::Config, for example
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub playlists: Vec<PlaylistConfig>,
    pub database: crate::database::Config,

    pub spotify: ModuleConfig<crate::service::spotify::Config>,
    pub discord: ModuleConfig<crate::service::discord::Config>,
    pub matrix: ModuleConfig<crate::service::matrix::Config>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModuleConfig<T> {
    #[serde(default)]
    pub enabled: bool,
    #[serde(flatten)]
    pub config: Option<T>,
}

impl<T: std::fmt::Debug> ModuleConfig<T> {
    pub fn get(&self) -> Option<&T> {
        if self.enabled {
            Some(self.config.as_ref().expect(&format!(
                "module enabled but no config {}",
                type_name::<T>()
            )))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlaylistConfig {
    pub name: Option<String>,
    pub id: Option<String>,
    pub desc: Option<String>,

    #[serde(default)]
    pub create: bool,
}

#[derive(Debug, Clone, clap::Parser)]
pub struct ConfigCli {
    /// Config path
    #[arg(
        short,
        long,
        env = "GOONTUNES_CONFIG",
        default_value = "~/.config/goontunes"
    )]
    pub config_path: String,

    /// Overrides to config
    #[arg(short = 'X')]
    pub config_overrides: Vec<String>,

    /// Overrides to config
    #[arg(short = 'J')]
    pub config_overrides2: Vec<String>,
}

pub fn load(cc: ConfigCli) -> Result<AppConfig, eyre::Error> {
    let mut s = config::Config::builder()
        // Start off by merging in the "default" configuration file
        .add_source(config::File::with_name(&cc.config_path))
        .add_source(config::Environment::with_prefix("goontunes").separator("__"));

    for o in cc.config_overrides {
        let mut spl = o.split("=");
        let a = spl.next().unwrap();
        let mut b = spl.next().unwrap().to_string();
        if regex::Regex::new(r#"^[^"'{}\]\[]*$"#).unwrap().is_match(&b) {
            b = format!("\"{}\"", b);
        }
        let b = format!(r#"{{ "_v" :  {} }}"#, b);
        let parse = config::FileFormat::Json5.parse(None, &b).unwrap();
        let b = parse.get("_v").unwrap();
        s = s.set_override(a, b.clone())?;
    }

    let c = s.build()?;

    let mut j: serde_json::Value = c.clone().try_deserialize().unwrap();
    let old = j.clone();

    //println!("{}", serde_json::to_string_pretty(&j)?);

    for o in cc.config_overrides2 {
        j = process_jq(j.clone(), o);
    }

    let diff = serde_json_diff::values(old, j.clone());
    dbg!(diff);

    let c: AppConfig = serde_json::from_value(j).unwrap();

    dbg!(&c);

    Ok(c)
}

fn process_jq(input: serde_json::Value, filter: String) -> serde_json::Value {
    use jaq_core::{load, Compiler, Ctx, Error, FilterT, Native, RcIter};
    use jaq_json::Val;
    use serde_json::{json, Value};

    let program = File {
        path: "".to_string(),
        code: filter.as_str(),
    };

    use load::{Arena, File, Loader};

    // start out only from core filters,
    // which do not include filters in the standard library
    // such as `map`, `select` etc.
    let loader = Loader::new(jaq_std::defs());
    let arena = Arena::default();

    // parse the filter
    let modules = loader.load(&arena, program).unwrap();

    // compile the filter
    let filter = jaq_core::Compiler::default()
        .with_funs(jaq_std::funs())
        .compile(modules)
        .unwrap();

    let inputs = RcIter::new(core::iter::empty());

    // iterator over the output values
    let mut out = filter.run((Ctx::new([], &inputs), Val::from(input)));
    out.next().unwrap().unwrap().into()
}
