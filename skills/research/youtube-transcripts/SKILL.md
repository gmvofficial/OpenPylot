---
name: youtube-transcripts
description: "Fetch and summarize YouTube video transcripts. Use when: user asks to summarize, transcribe, extract content, or get captions from a YouTube video URL or ID. Also use when asked to analyze, quote, or reference a YouTube video's spoken content."
version: '1.0.0'
author: pylot-team
category: research
tags:
  - youtube
  - transcript
  - video
  - summarize
  - captions
  - subtitles
examples:
  - 'Summarize this YouTube video: https://youtube.com/watch?v=...'
  - 'What does this video talk about?'
  - 'Get the transcript of this YouTube link'
requires:
  bins: [python3, bash]
---

# YouTube Transcripts

Fetch transcripts from YouTube videos using the bundled script. **No manual setup needed** — it auto-installs dependencies on first run.

**IMPORTANT: Do NOT use web_search, web_extract, or raw curl for YouTube transcripts. Use the `bash` tool to run the wrapper script below — it handles everything automatically.**

## Step 1: Fetch Transcript (use `bash` tool)

```bash
bash skills/research/youtube-transcripts/scripts/run.sh VIDEO_ID_OR_URL
```

With specific languages:

```bash
bash skills/research/youtube-transcripts/scripts/run.sh VIDEO_ID_OR_URL "en,fr,de"
```

The wrapper script automatically:

1. Creates a Python virtual environment (`.venv`) on first run
2. Installs `youtube-transcript-api` and `requests` into it
3. Runs the transcript fetcher
4. Subsequent runs skip setup and execute instantly

**Output:** JSON with these fields:

- `video_id` — the 11-character ID
- `title` — video title
- `author` — channel name
- `entries` — number of transcript segments
- `full_text` — **all caption text joined together (use this for summarization)**
- `transcript` — array of `{text, start, duration}` entries with timestamps

## Step 3: Present Results

Structure the output as:

- **Video**: Title by Author
- **Full Transcript**: The `full_text` field from the JSON output
- **Summary**: If user asked for a summary, summarize the `full_text` content
- Quote specific timestamps when referencing parts of the video

## When Transcript Is Unavailable

| Error                      | Meaning                                | Action                                              |
| -------------------------- | -------------------------------------- | --------------------------------------------------- |
| `Missing dependency`       | `youtube-transcript-api` not installed | Run: `pip3 install youtube-transcript-api requests` |
| `Transcripts are disabled` | Creator turned off captions            | Inform user, no workaround                          |
| `No transcript found`      | No captions in requested languages     | Try: `"fr,de,es,ja"` as second arg                  |
| Connection error on cloud  | YouTube blocks cloud IPs               | Set `USE_VPN=true` env var + configure WireGuard    |

## Tips

- The script works directly on local/home networks — no VPN needed
- For cloud servers (AWS, Hetzner, etc.), set env vars `USE_VPN=true` and `VPN_SOURCE_IP`
- For long videos, summarize key sections rather than dumping entire transcript
- Default language priority: en, fr, de, es, it, pt, nl
