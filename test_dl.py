import yt_dlp, os

opts = {
    "format": "bestaudio/best",
    "restrictfilenames": True, 
    "outtmpl": os.path.join(os.getcwd(), "%(title)s.%(ext)s"),
    "noplaylist": True,
    "quiet": False, 
    "postprocessors": [{
        "key": "FFmpegExtractAudio",
        "preferredcodec": "mp3",
        "preferredquality": "192",
    }],
    "postprocessor_args": {
        "FFmpegExtractAudio": [
            "-ar", "44100",
            "-ac", "2",
            "-b:a", "192k",
        ]
    },
    "writethumbnail": False,
}

from engine import probe_soundcloud
print("Testing ytdlp")
try:
    with yt_dlp.YoutubeDL(opts) as ydl:
        ydl.download(["https://www.youtube.com/watch?v=BaW_CjEpG7o"])
except Exception as e:
    print("ERROR:", e)

