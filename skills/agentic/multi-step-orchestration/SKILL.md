---
name: multi-step-orchestration
description: Break down complex multi-step tasks and coordinate their execution
version: "1.0.0"
category: agentic
tags:
  - orchestrate
  - multi-step
  - complex
  - delegate
  - coordinate
  - agent
  - parallel
---

# Multi-Step Orchestration Skill

When handling complex tasks that require multiple steps:

## Decomposition
1. Identify all discrete sub-tasks needed to complete the request
2. Map dependencies between sub-tasks (what needs what)
3. Identify which sub-tasks can run in parallel
4. Order execution for maximum efficiency

## Execution Strategy
- **Sequential**: When output of step N is input to step N+1
- **Parallel**: When steps are independent — execute simultaneously
- **Conditional**: When the next step depends on a previous result

## Progress Tracking
- Report progress after each major step
- If a step fails, diagnose and either retry or adjust the plan
- Summarize results from all steps at the end

## Communication
- Before starting: outline the plan and get confirmation for complex operations
- During execution: brief status updates on long-running tasks
- After completion: summary of what was done, any issues encountered, and results
