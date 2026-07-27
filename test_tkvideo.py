#!/usr/bin/env python3
"""Quick prototype to test tkvideo integration with tkinter."""
import tkinter as tk
from tkvideo import tkvideo
import os

TEST_VIDEO = "/Users/ty/Music/21 MINUTE YOGA WORKOUT  Full Body Strength & Muscle Building Sequence! - Kailasa's Nithyananda Yogis (720p, h264, youtube).mp4"

def test_basic_playback():
    root = tk.Tk()
    root.title("tkvideo Test")
    root.geometry("800x600")
    root.configure(bg="#1a1a1a")
    
    # Video label
    video_label = tk.Label(root, bg="#000")
    video_label.pack(fill="both", expand=True, padx=20, pady=20)
    
    # Controls
    ctrl_frame = tk.Frame(root, bg="#1a1a1a")
    ctrl_frame.pack(fill="x", padx=20, pady=(0, 20))
    
    player = None
    
    def on_play():
        nonlocal player
        if player:
            player.stop()
        player = tkvideo(TEST_VIDEO, video_label, loop=1, size=(760, 420))
        player.play()
    
    def on_pause():
        if player:
            player.pause()
    
    def on_stop():
        if player:
            player.stop()
            video_label.config(image="", text="Stopped")
    
    tk.Button(ctrl_frame, text="▶ Play", command=on_play).pack(side="left", padx=5)
    tk.Button(ctrl_frame, text="⏸ Pause", command=on_pause).pack(side="left", padx=5)
    tk.Button(ctrl_frame, text="⏹ Stop", command=on_stop).pack(side="left", padx=5)
    
    # Test seeking
    def on_seek(val):
        if player:
            player.seek(float(val))
    
    seek_slider = tk.Scale(ctrl_frame, from_=0, to=100, orient="horizontal", 
                           command=on_seek, bg="#1a1a1a", fg="white", 
                           highlightthickness=0, length=300)
    seek_slider.pack(side="left", padx=20)
    
    # Update slider periodically
    def update_seek():
        if player and player.player:
            try:
                pos = player.player.get_position()
                duration = player.player.get_duration()
                if duration > 0:
                    seek_slider.set((pos / duration) * 100)
            except:
                pass
        root.after(500, update_seek)
    
    update_seek()
    
    root.mainloop()

if __name__ == "__main__":
    if os.path.exists(TEST_VIDEO):
        test_basic_playback()
    else:
        print(f"Test video not found: {TEST_VIDEO}")
