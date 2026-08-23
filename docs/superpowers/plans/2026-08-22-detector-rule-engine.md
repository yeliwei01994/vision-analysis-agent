# Detector and Event Rule Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将视频处理骨架升级为可替换的检测器接口和可测试的事件规则引擎，为接入真实 YOLO、跟踪器和事件配置做好边界。

**Architecture:** `Detector` 接收抽象帧并返回统一 `Detection`，规则引擎只消费检测序列和规则配置，Worker 负责把视频任务编排成检测帧和事件。第三阶段默认使用 MockDetector，真实 YOLO 通过后续 ONNX/TensorRT 或 HTTP Adapter 接入。

**Tech Stack:** Rust, Serde, Tokio, Axum, React, TypeScript.

**Spec:** `2026-08-21-video-event-retrieval-development.md`

## Global Constraints

- 规则判断必须可重复、可单元测试，不让大模型直接决定事件是否产生。
- 检测器输出统一包含类别、置信度、边界框、帧时间和可选轨迹 ID。
- 事件类型和规则条件使用可扩展字符串/结构，不绑定某个 YOLO 类别编号。
- 本阶段不引入重量级推理依赖；真实 YOLO 通过 Adapter 接口预留。

### Task 1: Detector and rule contract

**Files:** `backend/src/adapters.rs`, `backend/src/rules.rs`, `backend/tests/rule_engine.rs`

- [ ] Write failing tests for confidence filtering and minimum dwell time.
- [ ] Run tests and observe missing rule engine types.
- [ ] Implement `Detector`, `FrameDetection`, `EventRule`, and `RuleEngine`.
- [ ] Run tests and verify they pass.

### Task 2: Worker integration

**Files:** `backend/src/worker.rs`, `backend/src/application.rs`, `backend/src/domain.rs`

- [ ] Write a failing test proving processing creates an event from a detector result.
- [ ] Implement `MockDetector` and inject it into the processing flow.
- [ ] Store detector and rule versions in the generated event.
- [ ] Run the full Rust suite.

### Task 3: Rule configuration API

**Files:** `backend/src/api.rs`, `backend/src/application.rs`, `backend/tests/api_contract.rs`

- [ ] Add `GET /api/v1/event-rules` and `PUT /api/v1/event-rules/{event_type}`.
- [ ] Validate non-negative duration and confidence range.
- [ ] Add API tests for default rules and updates.

### Task 4: Frontend rule visibility

**Files:** `frontend/src/api/client.ts`, `frontend/src/App.tsx`, `frontend/src/App.test.tsx`

- [ ] Add a compact active-rule indicator to the event workspace.
- [ ] Keep editing UI out of this phase; only display the active rule version.
- [ ] Run frontend tests and production build.

### Task 5: Verification and handoff

- [ ] Run Rust tests and formatting check.
- [ ] Run frontend tests and build.
- [ ] Run Compose config validation.
- [ ] Document the exact adapter contract for real YOLO integration.

