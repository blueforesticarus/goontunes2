use std::{any::type_name, fmt::Display};

use color_eyre::owo_colors::OwoColorize;
use config::Format;
use itertools::Itertools;
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

    #[arg(long)]
    pub print_config: bool,
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

    // if let Some(diff) = serde_json_diff::values(j.clone(), old) {
    //     dbg!(diff);
    // }
    let diffs = json_diff_ng::compare_serde_values(&j, &old, true, &[]).unwrap();
    for (d_type, d_path) in diffs.all_diffs() {
        // TODO this has bug where path shows 0 index for appened items
        let path = d_path.path.iter().map(ToString::to_string).join(".");
        let oldv = d_path
            .values
            .map_or_else(|| d_path.resolve(&j), |v| Some(v.1))
            .map(ToString::to_string)
            .unwrap_or_default();
        let newv = d_path
            .values
            .map_or_else(|| d_path.resolve(&old), |v| Some(v.0))
            .map(ToString::to_string)
            .unwrap_or_default();

        match d_type {
            json_diff_ng::DiffType::RootMismatch => todo!(),
            json_diff_ng::DiffType::LeftExtra => {
                println!("+ {} {}", path, newv);
            }
            json_diff_ng::DiffType::RightExtra => {
                println!("- {} {}", path, oldv);
            }
            json_diff_ng::DiffType::Mismatch => {
                println!("+ {} {}", path, newv);
                println!("- {} {}", path, oldv);
            }
        }
    }
    let txt = serde_json::to_string_pretty(&j).unwrap();
    if cc.print_config {
        println!("-------CONFIG-------");
        println!("{}", txt);
        std::process::exit(0);
    }

    match serde_json::from_value::<AppConfig>(j.clone()) {
        Ok(c) => {
            dbg!(&c);
            return Ok(c);
        }
        Err(e) => {
            println!("-------CONFIG-------");
            println!("{}", txt);
            println!("-------ERROR--------");
            panic!("{}", e);
        }
    };
}

fn process_jq(input: serde_json::Value, filter: String) -> serde_json::Value {
    use jaq_core::{load, Ctx, FilterT, RcIter};
    use jaq_json::Val;

    let program = File {
        path: "".to_string(),
        code: filter.as_str(),
    };

    use load::{Arena, File, Loader};

    // let funcs = load::parse("def new(v): . += [{} | v];", |p| p.defs())
    //     .unwrap()
    //     .into_iter();
    let loader = Loader::new(jaq_std::defs()); //.chain(funcs));
    let arena = Arena::default();

    // parse the filter
    let modules = loader.load(&arena, program).unwrap();

    // compile the filter
    let filter = jaq_core::Compiler::default()
        .with_funs(jaq_std::funs().chain(jaq_json::funs()))
        .compile(modules)
        .unwrap();

    let inputs = RcIter::new(core::iter::empty());

    // iterator over the output values
    let out: Vec<_> = filter
        .run((Ctx::new([], &inputs), Val::from(input)))
        .collect_vec();
    for a in out.iter() {
        match a {
            Ok(v) => println!("{}", v),
            Err(e) => println!("{}, {:#?}", e, e),
        }
    }
    out.get(0).unwrap().clone().unwrap().into()
}
