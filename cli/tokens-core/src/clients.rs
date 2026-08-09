#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRoot {
    Home,
    XdgData,
    Config,
    EnvVar {
        var: &'static str,
        fallback_relative: &'static str,
    },
}

impl PathRoot {
    pub fn resolve_with_env_strategy(&self, home_dir: &str, use_env_roots: bool) -> String {
        match self {
            PathRoot::Home => home_dir.to_string(),
            PathRoot::XdgData => {
                if use_env_roots {
                    std::env::var("XDG_DATA_HOME")
                        .unwrap_or_else(|_| format!("{}/.local/share", home_dir))
                } else {
                    format!("{}/.local/share", home_dir)
                }
            }
            PathRoot::Config => {
                if use_env_roots {
                    if let Some(custom) = std::env::var_os("TOKENS_CONFIG_DIR") {
                        if !custom.is_empty() {
                            return custom.to_string_lossy().into_owned();
                        }
                    }

                    #[cfg(target_os = "linux")]
                    if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
                        return format!("{xdg_config_home}/tokens");
                    }

                    // Match paths::get_config_dir() so default Windows scans
                    // read the same %APPDATA% root used by cache writers.
                    #[cfg(target_os = "windows")]
                    if let Some(dir) = dirs::config_dir() {
                        return dir.join("tokens").to_string_lossy().into_owned();
                    }
                }

                #[cfg(target_os = "windows")]
                if !use_env_roots {
                    return std::path::Path::new(home_dir)
                        .join("AppData/Roaming/tokens")
                        .to_string_lossy()
                        .into_owned();
                }

                format!("{home_dir}/.config/tokens")
            }
            PathRoot::EnvVar {
                var,
                fallback_relative,
            } => {
                if use_env_roots {
                    let val = std::env::var(var).unwrap_or_default();
                    if val.trim().is_empty() {
                        format!("{}/{}", home_dir, fallback_relative)
                    } else {
                        val
                    }
                } else {
                    format!("{}/{}", home_dir, fallback_relative)
                }
            }
        }
    }

    pub fn resolve(&self, home_dir: &str) -> String {
        self.resolve_with_env_strategy(home_dir, true)
    }
}

#[derive(Debug, Clone)]
pub struct ClientDef {
    pub id: &'static str,
    pub root: PathRoot,
    pub relative_path: &'static str,
    pub pattern: &'static str,
    pub headless: bool,
    pub parse_local: bool,
    pub submit_default: bool,
}

impl ClientDef {
    pub fn resolve_path_with_env_strategy(&self, home_dir: &str, use_env_roots: bool) -> String {
        format!(
            "{}/{}",
            self.root.resolve_with_env_strategy(home_dir, use_env_roots),
            self.relative_path
        )
    }

    pub fn resolve_path(&self, home_dir: &str) -> String {
        self.resolve_path_with_env_strategy(home_dir, true)
    }
}

macro_rules! define_clients {
    ( $( $variant:ident = $index:expr => { id: $id:expr, root: $root:expr, relative: $rel:expr, pattern: $pat:expr, headless: $hl:expr, parse_local: $pl:expr, submit_default: $sd:expr } ),+ $(,)? ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(usize)]
        pub enum ClientId {
            $( $variant = $index ),+
        }

        impl ClientId {
            pub const COUNT: usize = [ $( $index ),+ ].len();
            pub const ALL: [ClientId; Self::COUNT] = [ $( ClientId::$variant ),+ ];

            pub fn data(&self) -> &'static ClientDef {
                &CLIENTS[*self as usize]
            }

            pub fn as_str(&self) -> &'static str {
                self.data().id
            }

            pub fn file_pattern(&self) -> &'static str {
                self.data().pattern
            }

            pub fn supports_headless(&self) -> bool {
                self.data().headless
            }

            pub fn parse_local(&self) -> bool {
                self.data().parse_local
            }

            pub fn submit_default(&self) -> bool {
                self.data().submit_default
            }

            pub fn iter() -> impl Iterator<Item = ClientId> {
                Self::ALL.iter().copied()
            }

            #[allow(clippy::should_implement_trait)]
            pub fn from_str(s: &str) -> Option<ClientId> {
                Self::ALL.iter().copied().find(|c| c.as_str() == s)
            }
        }

        pub const CLIENTS: [ClientDef; ClientId::COUNT] = [
            $( ClientDef {
                id: $id,
                root: $root,
                relative_path: $rel,
                pattern: $pat,
                headless: $hl,
                parse_local: $pl,
                submit_default: $sd,
            } ),+
        ];

        const _: () = {
            let mut i = 0;
            $(
                assert!($index == i, "ClientId indices must be sequential");
                i += 1;
                let _ = i;
            )+
        };
    };
}

define_clients!(
    OpenCode = 0 => {
        id: "opencode",
        root: PathRoot::XdgData,
        relative: "opencode/storage/message",
        pattern: "*.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Claude = 1 => {
        id: "claude",
        root: PathRoot::Home,
        relative: ".claude/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Codex = 2 => {
        id: "codex",
        root: PathRoot::EnvVar {
            var: "CODEX_HOME",
            fallback_relative: ".codex",
        },
        relative: "sessions",
        pattern: "*.jsonl",
        headless: true,
        parse_local: true,
        submit_default: true
    },
    Cursor = 3 => {
        id: "cursor",
        root: PathRoot::Home,
        relative: ".config/tokens/cursor-cache",
        pattern: "usage*.csv",
        headless: false,
        parse_local: false,
        submit_default: true
    },
    Gemini = 4 => {
        id: "gemini",
        root: PathRoot::EnvVar {
            var: "GEMINI_CLI_HOME",
            fallback_relative: ".gemini",
        },
        relative: "tmp",
        pattern: "*.json|*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Amp = 5 => {
        id: "amp",
        root: PathRoot::XdgData,
        relative: "amp/threads",
        pattern: "T-*.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Droid = 6 => {
        id: "droid",
        root: PathRoot::Home,
        relative: ".factory/sessions",
        pattern: "*.settings.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    OpenClaw = 7 => {
        id: "openclaw",
        root: PathRoot::Home,
        relative: ".openclaw/agents",
        pattern: "*.jsonl*",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Pi = 8 => {
        id: "pi",
        root: PathRoot::Home,
        relative: ".pi/agent/sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Kimi = 9 => {
        id: "kimi",
        root: PathRoot::Home,
        relative: ".kimi/sessions",
        pattern: "wire.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Qwen = 10 => {
        id: "qwen",
        root: PathRoot::Home,
        relative: ".qwen/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    RooCode = 11 => {
        id: "roocode",
        root: PathRoot::Home,
        relative: ".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/tasks",
        pattern: "ui_messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    KiloCode = 12 => {
        id: "kilocode",
        root: PathRoot::Home,
        relative: ".config/Code/User/globalStorage/kilocode.kilo-code/tasks",
        pattern: "ui_messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Mux = 13 => {
        id: "mux",
        root: PathRoot::Home,
        relative: ".mux/sessions",
        pattern: "session-usage.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Kilo = 14 => {
        id: "kilo",
        root: PathRoot::XdgData,
        relative: "kilo/kilo.db",
        pattern: "kilo.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Crush = 15 => {
        id: "crush",
        root: PathRoot::XdgData,
        relative: "crush/projects.json",
        pattern: "projects.json",
        headless: false,
        parse_local: true,
        submit_default: false
    },
    Hermes = 16 => {
        id: "hermes",
        root: PathRoot::EnvVar {
            var: "HERMES_HOME",
            fallback_relative: ".hermes",
        },
        relative: "state.db",
        pattern: "state.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Copilot = 17 => {
        id: "copilot",
        root: PathRoot::Home,
        relative: ".copilot/otel",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Goose = 18 => {
        id: "goose",
        root: PathRoot::XdgData,
        relative: "goose/sessions/sessions.db",
        pattern: "sessions.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Codebuff = 19 => {
        id: "codebuff",
        root: PathRoot::EnvVar {
            var: "CODEBUFF_DATA_DIR",
            fallback_relative: ".config/manicode",
        },
        relative: "projects",
        pattern: "chat-messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Antigravity = 20 => {
        id: "antigravity",
        root: PathRoot::Config,
        relative: "antigravity-cache/sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Zed = 21 => {
        id: "zed",
        root: PathRoot::XdgData,
        relative: "zed/threads/threads.db",
        pattern: "threads.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Kiro = 22 => {
        id: "kiro",
        root: PathRoot::Home,
        relative: ".kiro/sessions/cli",
        pattern: "*.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Trae = 23 => {
        id: "trae",
        root: PathRoot::Config,
        relative: "trae-cache/sessions",
        pattern: "*.json",
        headless: false,
        parse_local: true,
        submit_default: false
    },
    Warp = 24 => {
        id: "warp",
        root: PathRoot::Config,
        relative: "warp-cache",
        pattern: "usage*.json",
        headless: false,
        parse_local: true,
        submit_default: false
    },
    Cline = 25 => {
        id: "cline",
        root: PathRoot::Home,
        relative: ".config/Code/User/globalStorage/saoudrizwan.claude-dev/tasks",
        pattern: "ui_messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Gjc = 26 => {
        id: "gjc",
        root: PathRoot::EnvVar {
            var: "GJC_CODING_AGENT_DIR",
            fallback_relative: ".gjc/agent",
        },
        relative: "sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Grok = 27 => {
        id: "grok",
        root: PathRoot::EnvVar {
            var: "GROK_HOME",
            fallback_relative: ".grok",
        },
        relative: "sessions",
        pattern: "updates.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Jcode = 28 => {
        id: "jcode",
        root: PathRoot::EnvVar {
            var: "JCODE_HOME",
            fallback_relative: ".jcode",
        },
        relative: "sessions",
        pattern: "session_*.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    CommandCode = 29 => {
        id: "commandcode",
        root: PathRoot::Home,
        relative: ".commandcode/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    MiMoCode = 30 => {
        id: "micode",
        root: PathRoot::XdgData,
        relative: "mimocode",
        pattern: "*.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    // Antigravity CLI stores each conversation as a SQLite `.db` under
    // `~/.gemini/antigravity-cli/conversations/`. Unlike the IDE-backed
    // `Antigravity` client (which pulls usage from a running language server
    // over RPC and caches JSONL under the config dir), the CLI usage sits on
    // disk and is read directly — no RPC, no `antigravity sync` needed. Honors
    // `GEMINI_CLI_HOME` so a relocated Gemini home is picked up.
    AntigravityCli = 31 => {
        id: "antigravity-cli",
        root: PathRoot::EnvVar {
            var: "GEMINI_CLI_HOME",
            fallback_relative: ".gemini",
        },
        relative: "antigravity-cli/conversations",
        pattern: "*.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Junie = 32 => {
        id: "junie",
        root: PathRoot::Home,
        relative: ".junie/sessions",
        pattern: "events.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Zcode = 33 => {
        id: "zcode",
        root: PathRoot::Home,
        relative: ".zcode/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    OpenCodeReview = 34 => {
        id: "opencodereview",
        root: PathRoot::Home,
        relative: ".opencodereview/sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    CodeBuddy = 35 => {
        id: "codebuddy",
        root: PathRoot::Home,
        relative: ".codebuddy/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    WorkBuddy = 36 => {
        id: "workbuddy",
        root: PathRoot::Home,
        relative: ".workbuddy",
        pattern: "workbuddy.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    DevinCli = 37 => {
        id: "devin-cli",
        root: PathRoot::XdgData,
        relative: "devin/cli/sessions.db",
        pattern: "sessions.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    DevinDesktop = 38 => {
        id: "devin-desktop",
        root: PathRoot::Home,
        relative: "Library/Application Support/Devin/User/acp-events",
        pattern: "*.ndjson",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    // Freebuff is a compile-time build variant of the Codebuff CLI, so it
    // writes to the same `~/.config/manicode*` tree and the same
    // `projects/<project>/chats/<chatId>/chat-messages.json` layout. The two
    // products are told apart per chat by the persisted root agent id, not by
    // location (see `sessions::freebuff`).
    Freebuff = 39 => {
        id: "freebuff",
        root: PathRoot::EnvVar {
            var: "FREEBUFF_DATA_DIR",
            fallback_relative: ".config/manicode",
        },
        relative: "projects",
        pattern: "chat-messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    }
);

pub struct ClientCounts {
    counts: [i32; ClientId::COUNT],
}

impl ClientCounts {
    pub fn new() -> Self {
        Self {
            counts: [0; ClientId::COUNT],
        }
    }

    pub fn get(&self, client: ClientId) -> i32 {
        self.counts[client as usize]
    }

    pub fn set(&mut self, client: ClientId, value: i32) {
        self.counts[client as usize] = value;
    }

    pub fn add(&mut self, client: ClientId, value: i32) {
        self.counts[client as usize] += value;
    }
}

impl Default for ClientCounts {
    fn default() -> Self {
        Self::new()
    }
}

