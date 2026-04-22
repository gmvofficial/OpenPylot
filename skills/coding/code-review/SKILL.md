---
name: code-review
description: Systematic code review with focus on correctness, security, and maintainability
version: "1.0.0"
category: coding
tags:
  - review
  - code
  - quality
  - security
  - lint
---

# Code Review Skill

When reviewing code, follow this structured approach:

## Review Checklist

1. **Correctness**: Does the code do what it's supposed to? Check edge cases, off-by-one errors, null/None handling.
2. **Security**: Look for injection vulnerabilities, hardcoded secrets, improper input validation, missing authentication checks.
3. **Performance**: Identify unnecessary allocations, O(n²) loops, missing indexes, N+1 queries.
4. **Readability**: Are names clear? Is the code self-documenting? Are complex sections commented?
5. **Error Handling**: Are errors propagated correctly? Are they user-friendly?
6. **Testing**: Is the code testable? Are critical paths covered?

## Output Format

Provide feedback grouped by severity:
- **Critical**: Must fix before merge (bugs, security issues)
- **Important**: Should fix (performance, maintainability)  
- **Suggestion**: Nice to have (style, minor improvements)

For each finding, include the specific line/section and a concrete fix suggestion.
