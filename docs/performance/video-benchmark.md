# 视频性能基准测试

使用固定视频、固定模型和同一台机器比较不同 YOLO 批量大小与并发度。基准脚本会先执行一次 warm-up，再执行指定数量的正式测试，并输出 JSON 与 CSV。

## 运行

```powershell
.\scripts\benchmark-video-performance.ps1 `
  -VideoPath .\test-data\sample.mp4 `
  -Runs 3 `
  -Configurations '1:1,4:1,4:2,8:1'
```

脚本默认通过 Docker Compose 重建 worker，使每组配置真正生效。若 worker 已经由外部方式配置，可使用 `-SkipWorkerRestart`，但此时配置只会记录在结果中，不能保证已应用。

结果写入 `artifacts/performance/<timestamp>/results.json` 和 `results.csv`。每一组配置包含总耗时、P50 和 P95；`TotalMs` 是从上传请求开始到任务进入终态的墙钟时间。

## 推荐实验矩阵

| Batch | 并发 | 用途 |
|---:|---:|---|
| 1 | 1 | 基线 |
| 4 | 1 | 仅批量推理 |
| 4 | 2 | 批量加并发 |
| 8 | 1 | 更大 batch |
| 4 | 4 | 可选，检查 CPU 竞争 |

每次实验保持视频、模型、Docker 资源和 `YOLO_DEVICE` 一致。第一次运行只用于模型加载和缓存预热，不纳入统计。正式运行建议 3–5 次。

## 如何解读

- 加速比：`基线 P50 / 优化配置 P50`。
- 处理倍率：`视频时长 / TotalMs`。
- 若并发增加但 P95 上升，通常表示 CPU、内存或模型线程出现竞争。
- 事件数量、检测数量和状态必须与基线保持可接受的一致性；速度提升不能以漏检换取。
- 如果 `TotalMs` 改善有限，应查看 Worker 输出的 `performance_summary` JSON，定位 `frame_extract`、`yolo_inference`、`rule_evaluation` 和 `annotated_video_encode` 哪一段仍是瓶颈。
