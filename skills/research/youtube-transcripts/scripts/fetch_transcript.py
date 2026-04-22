#!/usr/bin/env python3
"""Fetch YouTube transcript. Works locally without VPN (residential IP).
On cloud servers, set USE_VPN=true and configure WireGuard (see SETUP.md).
"""

import sys
import json
import os
import re

try:
    from youtube_transcript_api import YouTubeTranscriptApi
    from youtube_transcript_api._errors import TranscriptsDisabled, NoTranscriptFound
except ImportError:
    print(json.dumps({
        "error": "Missing dependency. Install with: pip3 install youtube-transcript-api",
        "install_cmd": "pip3 install youtube-transcript-api requests"
    }))
    sys.exit(1)

LANGUAGES = ["en", "fr", "de", "es", "it", "pt", "nl"]


def extract_video_id(url_or_id):
    """Extract video ID from URL or return as-is."""
    patterns = [
        r"(?:v=|/v/|youtu\.be/|/embed/|/shorts/)([a-zA-Z0-9_-]{11})",
        r"^([a-zA-Z0-9_-]{11})$"
    ]
    for pattern in patterns:
        match = re.search(pattern, url_or_id)
        if match:
            return match.group(1)
    return url_or_id


def get_video_title(video_id):
    """Get video title via oembed (no API key needed)."""
    try:
        import requests
        resp = requests.get(
            f"https://noembed.com/embed?url=https://www.youtube.com/watch?v={video_id}",
            timeout=10
        )
        data = resp.json()
        return data.get("title", "Unknown"), data.get("author_name", "Unknown")
    except Exception:
        return "Unknown", "Unknown"


def fetch_transcript(video_id, languages=None):
    """Fetch transcript using youtube-transcript-api."""
    if languages is None:
        languages = LANGUAGES

    # If USE_VPN is set, bind to VPN IP for cloud servers
    use_vpn = os.environ.get("USE_VPN", "").lower() in ("true", "1", "yes")

    if use_vpn:
        try:
            import requests
            from requests.adapters import HTTPAdapter

            class SourceIPAdapter(HTTPAdapter):
                def __init__(self, source_ip, **kwargs):
                    self.source_ip = source_ip
                    super().__init__(**kwargs)

                def init_poolmanager(self, *args, **kwargs):
                    kwargs["source_address"] = (self.source_ip, 0)
                    super().init_poolmanager(*args, **kwargs)

            vpn_ip = os.environ.get("VPN_SOURCE_IP", "10.100.0.2")
            session = requests.Session()
            session.mount("http://", SourceIPAdapter(vpn_ip))
            session.mount("https://", SourceIPAdapter(vpn_ip))
            api = YouTubeTranscriptApi(http_client=session)
        except Exception:
            api = YouTubeTranscriptApi()
    else:
        api = YouTubeTranscriptApi()

    transcript = api.fetch(video_id, languages=languages)
    return [{"text": entry.text, "start": entry.start, "duration": entry.duration} for entry in transcript]


def main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Usage: fetch_transcript.py <video_id_or_url> [languages]"}))
        sys.exit(1)

    video_input = sys.argv[1]
    languages = sys.argv[2].split(",") if len(sys.argv) > 2 else LANGUAGES

    video_id = extract_video_id(video_input)
    title, author = get_video_title(video_id)

    try:
        transcript = fetch_transcript(video_id, languages)
        full_text = " ".join([entry["text"] for entry in transcript])

        print(json.dumps({
            "video_id": video_id,
            "title": title,
            "author": author,
            "entries": len(transcript),
            "full_text": full_text,
            "transcript": transcript
        }))
    except TranscriptsDisabled:
        print(json.dumps({"error": "Transcripts are disabled for this video", "video_id": video_id}))
        sys.exit(1)
    except NoTranscriptFound:
        print(json.dumps({"error": f"No transcript found in languages: {languages}", "video_id": video_id}))
        sys.exit(1)
    except Exception as e:
        print(json.dumps({"error": str(e), "video_id": video_id}))
        sys.exit(1)


if __name__ == "__main__":
    main()
