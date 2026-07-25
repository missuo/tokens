# Workers 部署配置

> 目标：Next.js 前端 + API 通过 OpenNext 跑在 Cloudflare Workers 上，数据库用 Neon，经 Hyperdrive 连接池接入。

当前 `tokens-staging` 是唯一在跑的 Workers 环境。`wrangler.jsonc` 里预置了 `env.production` 块，等切换时用。

## 1. 一次性资源

Workers Builds 只负责构建和部署，不创建存储资源，这两条要先手工执行一次：

```sh
cd web

# ISR / unstable_cache 的增量缓存载体
bunx wrangler r2 bucket create tokens-staging-cache

# Neon 连接池。密码通过环境变量传入，避免进入 shell 历史和进程列表。
export HYPERDRIVE_CONNECTION_STRING='postgresql://<user>:<password>@<host>/<db>'
bunx wrangler hyperdrive create tokens-staging-db \
  --connection-string "$HYPERDRIVE_CONNECTION_STRING" \
  --sslmode require \
  --caching-disabled
```

`--caching-disabled` 是有意为之：应用层已有 `unstable_cache` + `revalidateTag` 的精确失效机制，
Hyperdrive 再叠一层不感知失效信号的查询缓存，会导致提交数据后页面仍显示旧值。这里要的是它的
连接池和热连接，不是它的缓存。

创建完把返回的 Hyperdrive id 填进 `web/wrangler.jsonc` 的 `hyperdrive[0].id`。

当前 staging 的值：`35e35c2c7c3b4e5bb9cb590f2db79168` → Neon `ep-lingering-recipe-a6jxhbus.us-west-2`。

## 2. 部署方式

两条路径，都可用。

**手工部署**（改完想立刻看效果时）：

```sh
cd web
bun run cf:deploy          # = cf:build && wrangler deploy
```

构建要几分钟 —— OpenNext 先跑完 `next build`，再把产物打包成 Worker bundle。

**不要用 `opennextjs-cloudflare deploy`。** 它在部署前会跑 `populate-cache`：起一个名叫
`open-next-cache-populate` 的临时 Worker，用 `wrangler dev --remote` 式的隧道代理到
Cloudflare，再通过那条隧道把构建期的缓存条目写进 R2。隧道一旦不通就 500，整个部署中止 ——
而 R2 桶本身是好的（直接调 API 写入正常），降并发到 1 也一样失败。

跳过它没有副作用：那一步只是预热 ISR 缓存，不预热的话首次请求正常渲染并写入，结果一致。

**Workers Builds**（连 GitHub，推送即部署）：

Dashboard → Workers & Pages → `tokens-staging` → Settings → Builds → Connect。
Cloudflare 会自动生成构建所需的 API token，**不需要在 GitHub 里配置任何 secret**。

| 配置项 | 值 |
|---|---|
| Repository | `missuo/tokens` |
| Branch | `refactor/cloudflare-workers` |
| Root directory | `web` |
| Build command | `bun install && bun run typecheck && bun run scripts/migrate-prod.ts && bun run cf:build` |
| Deploy command | `bunx wrangler deploy` |
| Non-production branch deploy command | `bunx wrangler versions upload` |

Root directory 是 `web` —— 目录重构前叫 `packages/frontend`，填错会直接构建失败。

## 3. 环境变量：区分构建期和运行期

Workers Builds 的 "Build variables and secrets" **只在构建期可见，运行时取不到**。
两类必须分开配置，配错会导致运行时报 500。

**构建期变量**（Dashboard → Settings → Builds → Build variables）：

| 变量 | 用途 |
|---|---|
| `DATABASE_URL` | 仅供 `migrate-prod.ts` 执行 Drizzle 迁移 |
| `MIGRATE_TARGET` | 设为 `staging`，显式打开迁移开关（默认关闭，防止任意构建误改数据库） |

**运行期 secret**（用 `wrangler secret put`，交互式输入，不要写进命令行参数）：

```sh
cd web
bunx wrangler secret put GITHUB_CLIENT_ID
bunx wrangler secret put GITHUB_CLIENT_SECRET
bunx wrangler secret put CRON_SECRET
bunx wrangler secret put NEXT_PUBLIC_URL
```

运行期**不需要** `DATABASE_URL`：连接串由 Hyperdrive 绑定在运行时提供，
`src/lib/db/index.ts` 会优先读绑定，读不到才回落到环境变量。

GitHub OAuth 需要为每个环境单独建 OAuth App（回调地址指向该环境的域名），
不要跨环境复用，否则回调会跳错站。

## 4. 验证清单

部署后按顺序确认：

1. `bunx wrangler tail tokens-staging` 看有无启动期报错
2. 打开域名 → 榜单能渲染（验证 Hyperdrive 连通 + SSR）
3. 刷新同一页两次 → 第二次应命中 ISR 缓存（验证 R2 增量缓存）
4. 用 CLI 提交一次数据 → 榜单数字立即变化（验证 `revalidateTag` + 分片标签缓存）
5. `bunx wrangler check startup` → 确认启动时间在限额内（OpenNext 包体较大，值得测）
6. 手动触发 cron：`bunx wrangler dev --test-scheduled` 后访问 `/__scheduled`

当前 staging 地址：https://tokens-staging.missuo.workers.dev

## 5. 切到生产

`wrangler.jsonc` 的 `env.production` 块已就位，步骤：

1. 建生产资源：`tokens-prod-cache` 桶 + 指向生产 Neon 库的 Hyperdrive 配置，
   把 id 填进 `env.production.hyperdrive[0].id`
2. `pg_dump` 现有生产库导入生产 Neon
3. 为 `tokens` 这个 worker 配一遍第 3 节的运行期 secret
4. `wrangler deploy --env production`
5. 切 DNS，观察

自托管 Docker 的部署文件（`Dockerfile`、`compose.yaml`、`deploy-frontend.yml`）已经删除，
回滚需要从 git 历史恢复。
