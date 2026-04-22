---
name: debug-systematically
description: Systematic debugging approach for diagnosing and fixing software bugs
version: "1.0.0"
category: coding
tags:
  - debug
  - bug
  - error
  - fix
  - diagnose
  - troubleshoot
---

# Systematic Debugging Skill

When helping debug issues, follow this systematic approach:

## Step 1: Reproduce
- Clarify the exact steps to reproduce the issue
- Identify expected vs actual behavior
- Note the environment (OS, language version, dependencies)

## Step 2: Isolate
- Narrow down to the smallest reproducible case
- Check if the issue is in the user's code, a dependency, or configuration
- Use binary search on recent changes if the issue is a regression

## Step 3: Diagnose
- Read error messages carefully — they usually point to the cause
- Check logs at the point of failure
- Trace data flow from input to the point of failure
- Look for common patterns: null/undefined, type mismatches, race conditions, resource exhaustion

## Step 4: Fix
- Make the minimal change that fixes the root cause (not symptoms)
- Explain WHY the fix works
- Suggest a test case that would catch this bug in the future

## Step 5: Verify
- Confirm the fix resolves the original issue
- Check for regressions in related functionality
