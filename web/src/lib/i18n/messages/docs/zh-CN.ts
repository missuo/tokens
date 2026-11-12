const messages = {
  meta: {
    title: "文档 - Tokens",
    description:
      "在 macOS、Linux 或 Windows 上安装 Tokens CLI，并在 App Store 获取 iOS App。",
    ogTitle: "文档 — Tokens",
    ogImageSubtitle: "安装 Tokens CLI，或获取 iOS App。",
  },
  title: "文档",
  description: "把 AI 编程用量送上排行榜——在终端或手机上都可以。",

  commands: {
    copy: "复制命令：{command}",
    copied: "已复制 {command}",
    notes: {
      install: "安装",
      linkGitHub: "关联你的 GitHub 账号",
      submitAutomatically: "自动提交",
      submitUsage: "提交你的用量",
      authenticate: "登录认证",
      submitNow: "立即发送用量",
      submitInBackground: "在后台持续提交",
      submittedSoFar: "已提交的内容",
      everythingElse: "其余全部命令",
      goPrivate: "转为私密并打印一个读取令牌",
    },
  },

  install: {
    title: "安装 CLI",
    description:
      "CLI 会扫描你机器上已安装的 AI 编程客户端，在本地汇总用量，并且只提交汇总数据。",
    macosNote:
      "{brewServices} 会让一个后台代理保持运行，用量因此自动保持最新，无需你操心。",
    linuxNote:
      "{tokensServe} 在前台运行提交程序；配合一个 systemd unit，让它在重启后依然存活。",
    windowsNote:
      "直接从 npm 运行，不会全局安装任何东西。用一个计划任务（Scheduled Task）定时提交。",
  },

  iosApp: {
    title: "iOS App",
    description: "在手机上查看你的排名和用量，无需打开浏览器。",
    cardTitle: "Tokens iOS 版",
    download: "在 App Store 下载",
    liquidGlass:
      "App 完全围绕 Liquid Glass 构建，因此能呈现 iOS 本身的层次与半透明质感，而不是一个套了壳的网页。",
    features: {
      shareCards: {
        title: "分享卡片",
        body: "把一天、一个月或全部时间段的用量做成一张值得发布的卡片，并在设备本地渲染。",
      },
      homeWidgets: {
        title: "主屏幕小组件",
        body: "今日 Token 与你的排名，在后台保持更新。",
      },
      lockWidgets: {
        title: "锁屏小组件",
        body: "每日用量、累计总量与当前排名，无需解锁即可一眼读到。",
      },
    },
    steps: {
      install: "在你的 iPhone 上打开上面的 App Store 链接并安装 Tokens。",
      enterUsername:
        "打开 App，然后输入一个 GitHub 用户名。这里没有登录。如果你的主页是私密的，App 还会要求填写一个读取令牌，可在设置中创建。请填自己的用户名，否则小组件会显示别人的用量。",
      addWidgets: "长按主屏幕或锁屏，添加小组件。",
    },
  },

  thirdPartyApps: {
    title: "Awesome 第三方应用",
    description: "其他人基于 Tokens 开发的应用和插件。它们由各自的作者运营，有自己的支持渠道和隐私政策。",
    byline: "作者 {author}",
    bylineWithPlugin: "作者 {author}，{plugin}作者 {pluginAuthor}",
    tokensPlugin: "Tokens 插件",
    listYours: "想把你的项目列在这里？欢迎在 {repo} 创建 issue。",
    nowdex: {
      body: "Nowdex 可以在 Mac、iPhone 和 iPad 上查看 Codex、Claude、Cursor 等订阅额度还剩多少，支持主屏幕小组件和菜单栏。它也会在这些额度旁边显示你的 Tokens 用量（Token、花费和消息数）。它读取公开的资料 API，从不需要你的 Tokens 登录。",
    },
    metrix: {
      body: "Metrix.Bar 把你常看的数据做成仪表盘，放在 macOS 的菜单栏或 Windows、Linux 的系统托盘里。它的 Tokens 插件会显示你的 Token 用量、每日花费、模型分布和设备。如果你已经用 tokens login 登录，它会读取 CLI 保存的凭据；否则可以填 GitHub 用户名查看公开主页，或填读取令牌查看私密主页。",
    },
  },

  everyday: {
    title: "日常使用",
    description: "五条命令覆盖整个流程。",
  },

  privateMode: {
    title: "私密模式",
    description:
      "跟踪用量而不公开它。私密账号不会出现在排行榜上，它的主页、设备、徽章与嵌入卡片都会表现得像它不存在一样。",
    toggle: {
      title: "开启与关闭",
      body: "用 {flag} 登录，或者在一台已登录的机器上再运行一次，就能把这个账号切为私密。{settings} 里的私密模式开关效果相同，也是唯一能切回公开的方式：CI 里的提交令牌可以隐藏你的主页，却永远无法把它公开。登录状态下你仍然能看到自己的主页。",
    },
    readTokens: {
      title: "读取令牌",
      body: "读取令牌（{token}）能让持有者为私密主页解锁——iOS App、脚本，或者一张你仍想展示的徽章。把它作为 {header} 发送，或把 {query} 附加到徽章或嵌入 URL 后面。{loginCommand} 会打印第一个；更多令牌可以在设置里创建和撤销。它们只能读取——绝不提交或改动任何东西——而且只显示一次，之后仅以哈希形式保存。",
    },
    deviceNames: {
      title: "设备名称",
      body: "无论公开还是私密，别人永远只会看到被打码的设备名称——保留首尾各两个字符，例如 {example}——因为设备名往往就是主机名。在你自己主页和设置里，你能看到完整名称。",
    },
    mcpNames: {
      title: "MCP 服务器名称",
      body: "默认关闭。{enable} 会让每次提交附带该次提交所涵盖客户端中已配置的 MCP 服务器名称（Claude Code、Cursor、Kiro 的 MCP 配置，以及 OpenCode 技能）；只读取名称，也从不打开其他客户端的配置。{flag} 只对单次提交生效。共享的名称会完整显示在你的主页上。{disable} 停止发送，下一次提交会清除已保存的列表。隐私模式只是对他人隐藏这些名称；关闭该选项才会彻底不再发送。{status} 会列出具体读取的文件。",
    },
  },

  verifiedBadge: {
    title: "认证徽章",
    description:
      "排行榜上名字旁边的一个小对勾。它只说明这个账号是真实的、找得到的人——仅此而已。",
    howToGet: {
      title: "怎么拿到它",
      body: "在你的 GitHub 主页上至少添加 {links}——就是 {settings} 里的「社交账号」字段。任意两个都算：个人站点、X、LinkedIn、Mastodon、YouTube。规则就这一条。",
      links: "两个社交链接",
      settings: "GitHub 主页设置",
    },
    timing: {
      title: "它什么时候出现",
      body: "链接每天重新读取一次，时间为 03:20 UTC。现在添加，徽章会在下一次运行而不是立刻出现——退出再登录也不会更快。掉到两个链接以下，它会按同一时间表消失。",
    },
    why: {
      title: "为什么是两个链接",
      body: "排行榜会招来一次性账号。对本来就真实存在于网上的人来说，填两个社交字段轻而易举，而要大规模伪造则很繁琐——徽章能说明的仅此而已。它不是身份验证，对排名也没有任何影响——虚高的数字另有处理方式，由提交校验和{shame}负责。",
    },
  },

  clients: {
    title: "支持的客户端",
    description: "CLI 扫描你机器上已有的东西——无需按客户端做任何配置。",
    count:
      "这 {count} 个全部都会被自动识别——只要装了，并且写过会话，就会被计入。",
    paseo:
      "Paseo 的用量会归到它所依托的受支持提供商名下，例如 Claude Code 或 Codex CLI。Tokens 还会在 Paseo 的提供商配置中发现绝对路径的 Claude 与 Codex 数据目录覆盖项。要在 Paseo daemon 所在主机上运行 Tokens，才能提交那些会话。",
  },

  architecture: {
    title: "架构",
    description:
      "什么跑在哪里。仓库是公开的，所以这些都可以被核对，而不必凭信任接受。",
    site: {
      title: "站点与 API",
      body: "Next.js，通过 OpenNext 部署到 Cloudflare Workers——同一个 Worker 同时提供页面和 API，背后没有源站服务器。静态资源、分享卡片，以及未登录读者看到的页面都在边缘缓存，所以大多数请求根本不需要运行任何代码就得到应答。",
    },
    database: {
      title: "数据库",
      body: "{neon}（Postgres），经 Cloudflare Hyperdrive 访问，它会在数据库旁边维持预热的连接池，让一个发出多条查询的页面不必为每条都付一次全新的握手。Worker 被固定在同一区域：请求只跨一次洋，之后的每条查询都是本地跳转。结构变更走 Drizzle 迁移，在构建时应用。",
    },
    cli: {
      title: "CLI",
      body: "Rust 编写，通过 npm 按平台分发预编译二进制。它读取你的客户端本来就在写的会话文件，在机器上汇总，只发送汇总数据——Token 数、模型名、时间戳。提示词、回答和文件内容从不离开本机，MCP 服务器名称也只有在你主动开启后才会发送。{dryRun} 会原样打印将要上传的内容。",
    },
    caching: {
      title: "缓存与定时任务",
      body: "渲染好的页面存放在 R2 中，由 Durable Objects 跟踪一次提交会失效哪些标签——所以你自己的数字在你提交的那一刻就更新，而不是等某个定时器。每日徽章刷新以 Worker cron 触发器在进程内运行，不需要外部调度器持有密钥。",
    },
  },

  sponsors: {
    title: "赞助商",
    description:
      "两家公司为这个项目本不承担的基础设施付费。两家都没有要求任何回报，也都无权干涉排行榜报告什么。",
    vps: {
      title: "{vps} — 服务器",
      p1: "V.PS 由 {xtom} 运营，提供本站运行的东京服务器。",
      p2: "重点在于线路而不是硬件。读这个排行榜的人很大一部分在中国大陆，那里的出境路径比远端等待着的任何东西都重要——而再多的缓存也修不好一条通往缓存的慢路径。用他们自己的话说：每个套餐都走各运营商提供的最优路由，因此来自中国大陆的连接全天保持快速。",
      p3: "如果你也想要同样的东西，他们在 {vpsHosting} 出售。",
    },
    neon: {
      title: "{neon} — 数据库",
      p1: "Neon 提供承载本站每一次提交的无服务器 Postgres。",
      p2: "它换来的是不做近似的决定。排行榜每次读取都由真实的每日行计算，而不是来自一份更便宜、略微不准的夜间汇总；一次提交会在一分钟内改变你的位置；主页上的每个数字都能追溯到某一行，而不是某人记得去加一的计数器。Tokens 免费使用、也免费自托管，所以这些都不由使用它的人出钱。",
    },
  },
};

export default messages;
