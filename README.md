# 展会展品入馆封箱与撤展核对系统

展品从**进馆 → 布展 → 在场 → 撤展 → 出馆**全程逐件留痕。既能按箱接收，也能查到每件展品在场馆内的去向与最后经手人。

- **后端**：Rust + Axum 0.7 + SQLx + SQLite（流转账本 append-only）
- **前端**：Svelte 4 + Vite（展位看板、展品追溯时间线、扫码模拟）
- **照片**：异常现场照片存本地 `data/uploads/`，经 `/uploads/*` 提供访问
- **闸口**：`/api/gate/scan` 模拟扫码，支持箱号 / 展品编码 / 放行凭证（一次性）

## 一条命令本地启动

```bash
./run.sh          # 构建前端并启动，浏览器打开 http://localhost:8080
./run.sh dev      # 开发模式：Vite 5173 热更新 + 后端 8080
```

> 需要 Rust（stable，缺失时脚本自动用 rustup 安装）和 Node 18+。
> 首次启动自动建库（`data/expo.db`）并写入演示人员、参展商、展位。
> 可用环境变量：`PORT`、`DATABASE_URL`、`UPLOAD_DIR`（见 `.env.example`）。

## 演示身份（顶栏可随时切换）

| 身份 | 账号 | 权限 |
|---|---|---|
| 甲方参展商-王展 | `u-alpha` | 提交清单、发起变更申请；**无权**核销异常/签发放行 |
| 乙方参展商-李博 | `u-beta` | 同上 |
| 入口闸口-赵检 | `u-gate` | 按箱接收、撤展逐件核验、闸口扫码、单件出馆 |
| 场馆布展-孙班 | `u-venue` | 布展占位/转移/撤下、登记缺件破损 |
| 场馆经理-周总 | `u-mgr` | 审批变更（替换/转借/提前撤展）、核销异常、签发凭证 |

演示展位：`A1`、`A2`（甲方）、`B1`（乙方）。

## 业务流程

```
参展商逐件清单 ──► 入口按箱接收(封箱,关联箱内每件) ──► 布展登记展位
                                                      │
变更: 临时替换/转借展位/提前撤展 ──► 经理审批后生效（原记录保留）
                                                      │
撤展: 按原清单逐件核实实物 ──► 缺件/破损登记+现场照片 ──► 经理核销
                                                      │
                              入馆=在场累计=出馆+核销 ──► 经理签发《出馆放行凭证》
                                                      │
                                          闸口扫码(一次性) ──► 放行
```

### 关键规则（均由后端强制，前端无法绕过）

1. **同一件展品不能同时出现在两个展位**：`placements` 上有部分唯一索引
   `WHERE active=1`，应用层 + 数据库双重拦截，重复占位返回 **409**；转移展位时旧占位失效、新占位生效。
2. **入馆、在场、出馆数量必须相符**：签发放行凭证时核对
   `申报数 == 实际入馆数 == 已出馆 + 经理核销数`，且无未处理异常，否则 **409** 拒绝签发。
3. **参展商无权自行批准异常放行**：异常核销（找到 / 缺件核销平账）和放行凭证签发
   只允许 `manager`，参展商/闸口/布展身份调用返回 **403**。
4. **不能直接改入馆记录**：清单不允许覆盖重报（400）；临时替换产生清单**新版本**，
   旧展品置为 `REPLACED` 但所有入馆/箱单/流转记录原样保留，新展品必须**重新按箱入馆核对**。
5. **缺件/破损**：未处理异常会阻止该件出馆；现场照片通过 multipart 上传存本地。
6. **转借展位**：审批后借用方才能把展品放上该展位，否则 400。
7. **闸口**：放行凭证扫码一次后再次使用 `deny`；无凭证的已出馆展品同样拒绝。

## 主要 API

| 方法/路径 | 身份 | 说明 |
|---|---|---|
| `POST /api/manifests` | 参展商 | 提交逐件清单（每参展商一次，变更走审批） |
| `GET  /api/manifests/:eid` | 本人/场馆 | 清单版本与逐件状态 |
| `POST /api/crates` | 闸口 | 按箱接收，`exhibit_codes` 关联箱内每件 |
| `GET  /api/exhibits` / `.../trace` | 任意 | 展品去向、流转时间线、异常、照片 |
| `POST /api/placements` | 布展 | 占位；重复占位 409 |
| `POST /api/exhibits/:code/transfer` | 布展 | 展位转移 |
| `POST /api/exhibits/:code/exception` | 闸口/布展 | 登记 missing/damaged |
| `POST /api/exhibits/:code/photo` | 任意 | 现场照片上传（multipart，本地存储） |
| `POST /api/exceptions/:id/resolve` | **经理** | 异常核销（仅经理，否则 403） |
| `POST /api/changes` `.../:id/review` | 参展商→**经理** | 替换/转借/提前撤展申请与审批 |
| `POST /api/verify/:eid` | 闸口/布展 | 撤展按清单逐件核实 |
| `POST /api/passes` | **经理** | 数量核对通过后签发凭证（否则 409；非经理 403） |
| `POST /api/gate/scan` | 闸口 | 扫码模拟：allow/deny + 原因，凭证一次性 |

所有请求用请求头 `X-User-Id: <账号>` 表明当前操作人（演示鉴权）。

## 测试

```bash
cargo test
```

9 个端到端集成测试（内存 SQLite + 完整 Axum 服务）覆盖：

- `duplicate_placement_is_rejected_but_transfer_works` —— **重复占位 409**，转移后仍只有一个生效占位
- `cannot_place_on_booth_not_owned_or_loaned` —— 未转借不能占位
- `unauthorized_roles_cannot_approve_or_issue_pass` —— **参展商/闸口/布展签发凭证 403，参展商自行核销异常 403**
- `full_quantities_must_balance_before_pass` —— **入馆=出馆=3 才签发**
- `missing_item_blocks_pass_until_manager_writes_off` —— 缺件阻止放行，经理核销平账后才签发
- `crate_cannot_be_received_twice` / `manifest_cannot_be_resubmitted_directly`
- `replacement_creates_new_manifest_version_and_keeps_history` —— 替换出新版本、旧记录保留、新件重新入馆
- `gate_pass_scan_is_one_time_only` —— 凭证扫码一次性、参展商不能扫

## 项目结构

```
migrations/0001_init.sql   SQLite 表结构（含部分唯一索引：一件展品一个生效展位）
src/
  main.rs / lib.rs         启动与路由
  handlers.rs              全部业务接口与规则校验
  state.rs                 连接池、X-User-Id 鉴权提取器（角色控制）
  db.rs                    建库/迁移/种子数据
  error.rs                 400/403/404/409 统一错误
frontend/src/pages/        Svelte 九个页面（总览/清单/箱接收/展位/追溯/异常/变更/撤展/闸口）
tests/api.rs               端到端集成测试
run.sh                     一条命令启动
data/                      SQLite 库与上传照片（运行时生成，已 gitignore）
```

## 设计说明

- **append-only 流转账本**：`movement_events` 每件展品一条递增 `seq` 链
  （申报→按箱入馆→占位→[转移/替换/异常]→核验出馆），每个事件带经手人，
  缺件时沿链即可定位**最后经手人**。
- **数量口径**：申报/入馆看 `exhibits.active` 与状态；出馆看 `status='OUT'`；
  缺件核销 `written_off` 计入去向平账。临时替换旧件 `active=0`，不影响新旧两版的账。
- **照片**：只存本地磁盘 + 数据库记录路径，不依赖外部对象存储。
