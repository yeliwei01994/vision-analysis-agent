# Vision Analysis Platform 设计文档

日期：2026-08-21  
状态：待审核  
范围：React 前端、FastAPI API、MySQL、Redis、Docker Compose、Nginx 独立部署

## 1. 目标与背景

当前项目是一个最小可用的视觉分析服务：FastAPI 接收图片，调用 YOLOv8 完成目标检测，聚合场景理解，并通过 DeepSeek 或本地模板生成报告。现有接口为 `POST /analyze`，项目没有前端应用、历史记录、数据库、缓存或完整容器编排。

本设计的目标是把它升级为一个可展示、可扩展、可独立部署的视觉分析平台：

- React 提供专业的分析工作台体验。
- FastAPI 保持推理和业务编排职责。
- MySQL 保存分析历史和业务数据，作为最终数据源。
- Redis 提供可重建缓存和重复分析去重。
- React 与 FastAPI 独立部署，生产环境由 Nginx 托管前端并反向代理 API。
- Docker Compose 负责本地和测试环境中的 API、MySQL、Redis 编排。

## 2. 非目标

本阶段不实现以下内容：

- 不把 YOLO 推理拆成独立模型服务。
- 不引入 Celery、Kafka、RabbitMQ 或 Kubernetes。
- 不实现多租户、登录权限和团队协作。
- 不实现 RTSP、视频流实时分析。
- 不把图片、标注图或 PDF 二进制存入 MySQL 或 Redis。
- 不在第一阶段强行引入对象存储、SSR 或 Next.js。

图片标注图、PDF 报告、视频流和告警规则作为后续扩展点保留，但不在当前第一版中伪装成已有能力。

## 3. 关键设计决策

### 3.1 前端技术路线

使用 React + Vite + TypeScript 构建独立前端应用。

- React：适合当前单页分析工作台和后续多页面扩展。
- Vite：构建简单、启动快，适合独立部署到 Nginx。
- TypeScript：让 API 响应、检测框和组件数据具备明确契约。
- TanStack Query：管理分析请求、缓存、加载状态和错误重试。
- Tailwind CSS + shadcn/ui：建立一致的视觉和交互基础，减少页面级样式堆积。
- 不引入全局状态库作为第一步；页面状态由 React 管理，服务端状态由 TanStack Query 管理。

### 3.2 API 兼容策略

保留当前 `POST /analyze` 作为兼容接口，不立即改名为 `/detect`。新增能力优先采用以下路径：

- `GET /health`：应用健康检查。
- `GET /history`：查询最近分析记录。
- `GET /history/{id}`：查询单条分析记录，必要时再实现。

未来如果接口数量扩大，再统一迁移到 `/api/v1`，并通过兼容期逐步弃用旧路径。前端通过单独的 API client 访问接口，不在组件中散落 `fetch` 调用。

### 3.3 数据源与缓存策略

MySQL 是分析历史的最终数据源。Redis 只保存可以重新计算或从 MySQL 重建的内容：

- 最近历史记录：短 TTL，写入新记录后失效。
- 相同图片和上下文的分析结果：使用图片内容 SHA-256 与分析上下文组成去重键。
- 后续统计数据：只作为展示加速，不作为事实来源。

Redis 读写失败时，核心分析和历史查询都必须继续工作；缓存异常只记录 warning，不把可选依赖升级成硬故障。

## 4. 系统架构

```text
Browser
  │
  ▼
Nginx
  ├── /                 → React 静态构建产物
  └── /api/             → FastAPI
                              │
                              ├── YOLOv8 推理
                              ├── 场景理解与报告编排
                              ├── MySQL：分析历史最终存储
                              └── Redis：历史缓存与重复分析去重
```

生产环境前端和 API 独立部署。Nginx 可以与前端构建产物放在同一台服务器，也可以单独作为网关；FastAPI、MySQL 和 Redis 在内网中通过服务名或私有地址通信。

开发环境允许 Vite 与 FastAPI 分开运行：Vite 使用 `/api` 代理到 FastAPI，避免前端代码依赖开发机上的跨域配置。生产环境由 Nginx 完成相同的路径转发。

## 5. 目标目录结构

```text
vision-analysis-agent/
├── app/
│   ├── main.py
│   ├── detection.py
│   ├── reporting.py
│   ├── deepseek_client.py
│   ├── schemas.py
│   ├── db_service.py          # 新增：MySQL 连接、模型和查询
│   └── cache_service.py       # 新增：Redis 客户端和缓存策略
├── frontend/
│   ├── src/
│   │   ├── api/
│   │   ├── components/
│   │   ├── features/analysis/
│   │   ├── features/history/
│   │   ├── lib/
│   │   ├── types/
│   │   └── App.tsx
│   ├── public/
│   ├── package.json
│   ├── vite.config.ts
│   └── nginx.conf
├── tests/
├── docker-compose.yml
├── Dockerfile
├── .env.example
└── README.md
```

前端按 feature 划分，而不是按页面堆放所有组件。分析页的业务组件和通用 UI 组件分离，后续增加历史详情、告警规则时不会把 `App.tsx` 变成单体文件。

## 6. 前端产品与交互设计

### 6.1 分析工作台

页面采用三栏工作台布局：

- 左侧导航：分析入口、历史记录、系统状态。
- 中央画布：拖拽上传、图片预览、分析状态和检测框叠加。
- 右侧结果面板：目标数量、类别统计、风险提示和分析报告。

核心流程：

1. 用户拖入或选择图片。
2. 前端校验图片类型和大小，并显示预览。
3. 用户可填写业务场景上下文。
4. 点击分析后展示加载态，禁止重复提交。
5. 分析成功后展示检测框、统计、风险和报告来源。
6. 结果可保存到近期记录；当前优先使用后端历史接口，接口不可用时只提示错误，不静默丢失关键结果。

### 6.2 检测框叠加

新增独立的 `DetectionOverlay` 组件，根据原图实际尺寸和容器尺寸计算比例，渲染 `xmin/ymin/xmax/ymax`。组件只消费图片尺寸和 `Detection[]`，不直接依赖 API 请求或页面布局。

当前后端只返回检测框坐标，不返回标注图片，因此第一版使用前端叠加渲染。后续如后端返回标注图，组件可以扩展为原图层与标注图层切换，而不改动分析页主体。

### 6.3 状态与错误

前端必须明确区分：空状态、上传中、分析中、成功、接口失败、图片格式失败和服务不可用。错误信息使用用户可理解的中文文案，同时保留可供开发排查的错误码或请求 ID位置。

## 7. API 与数据契约

### 7.1 当前分析接口

保留：

```text
POST /analyze
Content-Type: multipart/form-data
file: image file
context: optional string
```

响应继续使用当前 `AnalysisResponse`：

```json
{
  "context": "商场公共区域安全巡检",
  "detections": [],
  "scene": {
    "object_count": 0,
    "main_objects": [],
    "risk_notes": [],
    "observations": []
  },
  "report": "...",
  "report_source": "deepseek"
}
```

### 7.2 历史接口

第一版增加：

```text
GET /history?limit=20
```

历史记录至少包含：记录 ID、创建时间、上下文、目标统计、风险摘要、报告来源和报告文本。图片文件路径如果被持久化，必须使用应用可访问的相对路径或资源 URL，不暴露宿主机绝对路径。

### 7.3 版本与校验

Pydantic schema 继续作为 FastAPI 的响应校验层；前端维护镜像 TypeScript 类型。后续接口稳定后可生成 OpenAPI 类型，减少手工同步。

## 8. MySQL 数据设计

新增 `analysis_records` 表，建议字段：

- `id`：自增主键。
- `context`：业务场景，可为空。
- `filename`：原始文件名。
- `image_sha256`：用于去重和审计。
- `detections_json`：检测明细 JSON。
- `scene_json`：场景理解 JSON。
- `report`：报告文本。
- `report_source`：`deepseek` 或 `fallback`。
- `created_at`：创建时间。

第一版不把检测框拆成独立关系表，因为查询目标明细的场景尚未形成，JSON 更适合保留当前响应结构。对 `created_at` 和 `image_sha256` 建索引；如果未来需要按风险级别筛选，再新增结构化字段和索引。

使用 SQLAlchemy 2.x 管理连接和事务。`init_db()` 只创建不存在的表，不删除数据；数据库密码、连接地址全部来自环境变量。

## 9. Redis 设计

推荐键空间：

```text
vision:history:latest:{limit}
vision:analysis:{sha256}:{context_hash}
vision:stats:total
```

建议默认 TTL：历史 60 秒，重复分析 24 小时。缓存值使用 JSON 序列化。所有缓存操作封装在 `cache_service.py` 中，业务层不直接操作 Redis 客户端。

重复分析的正确性边界：只有在缓存完整保存了之前的响应，并且图片哈希、上下文和相关配置一致时才能直接返回；YOLO 模型或关键规则版本发生变化时，应通过版本前缀使旧缓存自然失效。

## 10. Docker 与 Nginx 部署

### 10.1 Docker Compose

Compose 至少包含：

- `vision-agent`：FastAPI API。
- `mysql`：MySQL 8，使用 volume 保存数据。
- `redis`：Redis 7，可使用 volume 保存必要状态。

应用容器通过 `mysql` 和 `redis` 服务名连接依赖，不能在容器环境中使用 `localhost`。MySQL 增加健康检查；应用启动时应对数据库连接失败给出清晰日志。

上传文件、后续标注图和报告文件使用明确的 volume 目录；Redis 不保存二进制文件。

### 10.2 Nginx

生产 Nginx 配置需要覆盖：

- React `index.html` 和静态资源缓存策略。
- `try_files` 支持前端 SPA 路由刷新。
- `/api/` 反向代理到 FastAPI。
- `client_max_body_size` 与后端上传限制保持一致。
- API 超时应覆盖 YOLO 推理和 DeepSeek 调用的合理上限。
- 添加基础安全响应头，不在前端暴露数据库和 DeepSeek 密钥。

## 11. 分阶段实施

### 阶段一：React 工作台

- 初始化 `frontend` Vite + TypeScript 项目。
- 建立 API client、类型、分析页和检测框叠加。
- 完成加载、成功、失败、空状态和响应式布局。
- 先接现有 `/analyze`，不等待数据库能力。

验收：可以独立启动前后端，上传图片并在 React 页面查看完整结果。

### 阶段二：接口工程化与部署链路

- 增加 `/health`。
- 完善前端代理、生产 Nginx 配置和 CORS 边界。
- 增加图片大小、类型、请求超时和异常响应处理。
- 增加 API 和前端基础测试。

验收：开发环境和 Nginx 生产构建都能完成一次完整分析。

### 阶段三：MySQL 历史记录

- 增加 SQLAlchemy、MySQL 配置和 `analysis_records`。
- 分析成功后持久化记录。
- 增加 `/history` 并接入前端历史列表。
- 为数据库异常增加降级和清晰错误日志。

验收：重启 API 后历史数据仍存在，前端可以打开最近分析记录。

### 阶段四：Redis 与 Compose

- 增加 Redis cache-aside、历史缓存和重复分析去重。
- 增加 Docker Compose、数据卷和服务健康检查。
- 验证 Redis 停止时 MySQL 路径仍可用。
- 补充 README、部署配置和集成测试。

验收：Compose 可启动 API、MySQL、Redis；缓存命中有效；Redis 故障不影响核心业务。

## 12. 测试策略

- 单元测试：场景理解、报告 fallback、缓存键、数据库序列化。
- API 测试：图片类型校验、分析成功、分析失败、健康检查、历史查询。
- 前端测试：上传交互、加载状态、错误状态、检测框比例计算。
- 集成测试：Compose 服务启动、MySQL 持久化、Redis 缓存未命中和降级。
- 手工验收：375px 移动端、桌面端、Nginx SPA 刷新、超大图片和无 DeepSeek Key 场景。

## 13. 风险与处理

### YOLO 首次加载慢

保留模型单例缓存；健康检查只检查应用进程，不把模型推理绑定到存活检查。前端明确展示首次分析可能较慢。

### DeepSeek 网络或 Key 不可用

继续使用已有 fallback 报告逻辑，并在结果中显示 `report_source`，让用户知道报告来源。

### Redis 与 MySQL 不一致

Redis 只作为缓存；新记录写入 MySQL 成功后失效历史缓存，缓存失败不回滚主业务。

### 同步请求阻塞

第一版继续使用同步分析接口，设置合理超时和单请求保护。只有当真实场景出现长耗时、并发或视频流需求时，再拆分异步任务队列。

### 文档与当前仓库不一致

外部实施计划中关于 SQLite、`/detect`、PDF 和已有数据库层的部分只作为目标方向参考；实施时以本设计和当前仓库代码为准，不直接假设不存在的文件或接口。

## 14. 最终验收标准

- React 前端可独立构建并由 Nginx 托管。
- Nginx 可将 `/api/` 转发到 FastAPI，刷新前端路由不返回 404。
- 用户可以上传图片、填写场景、发起分析，并查看检测框、统计、风险和报告。
- `/analyze` 兼容当前响应结构。
- 分析历史可持久化到 MySQL，并通过 `/history` 返回。
- Redis 命中时可复用历史或重复分析结果，Redis 不可用时核心功能仍可工作。
- Docker Compose 可以启动 FastAPI、MySQL 和 Redis，并使用 volume 保存持久数据。
- `.env`、数据库密码和 DeepSeek Key 不进入源码或前端构建产物。
- 原有报告逻辑测试继续通过，新增 API、数据库、缓存和前端关键流程具备测试覆盖。

