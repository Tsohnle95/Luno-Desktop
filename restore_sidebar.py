    def _populate_sidebar(self):
        """Redraws sidebar cards with right-click menu restored."""
        for w in self._pl_inner.winfo_children():
            w.destroy()
        self._pl_card_frames = []

        

        # Organize playlist order
        total_songs = sum(len(v) for v in self._library.values())
        entries = [("All Music", total_songs)]
        if "Unsorted" in self._library: entries.append(("Unsorted", len(self._library["Unsorted"])))
        for name in sorted(self._library.keys()):
            if name not in ["Unsorted", "All Music"]:
                entries.append((name, len(self._library[name])))

        for idx, (name, count) in enumerate(entries):
            card = tk.Frame(self._pl_inner, bg=SIDEBAR, cursor="hand2")
            card.pack(fill="x", pady=1)
            inner = tk.Frame(card, bg=SIDEBAR, cursor="hand2")
            inner.pack(fill="x", padx=8, pady=3)
            art = PlaylistArt(inner, 42, name, bg=SIDEBAR)
            art.pack(side="left", padx=(0, 10))
            txt_f = tk.Frame(inner, bg=SIDEBAR)
            txt_f.pack(side="left", fill="both", expand=True)
            nl = tk.Label(txt_f, text=name, font=("Helvetica Neue", 11, "bold"), fg=TEXT, bg=SIDEBAR, anchor="w")
            nl.pack(anchor="w")
            cl = tk.Label(txt_f, text=f"{count} songs", font=("Helvetica Neue", 9), fg=TEXT3, bg=SIDEBAR, anchor="w")
            cl.pack(anchor="w")

            def show_context(e, pl_name=name):
                m = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT, font=("Helvetica Neue", 12))
                if pl_name != "All Music":
                    m.add_command(label=f"Dump '{pl_name}' elsewhere...", command=lambda: self._ctx_dump_playlist(pl_name))
                    m.add_separator()
                    m.add_command(label=f"🧹 Empty Playlist", command=lambda: self._ctx_clear_playlist_contents(pl_name))
                    if pl_name != "Unsorted":
                        m.add_command(label=f"🗑 Delete Folder", command=lambda: self._ctx_delete_playlist_folder(pl_name))
                m.post(e.x_root, e.y_root)

            for w in [card, inner, art, nl, cl, txt_f]:
                w.bind("<Button-1>", lambda e, n=name, i=idx: self._select_playlist_card(i, n))
                w.bind("<Button-2>", show_context)
                w.bind("<Button-3>", show_context)
                w.bind("<Enter>", lambda e, c=card, inn=inner, n=nl, cll=cl, tx=txt_f: [x.config(bg=BG3) for x in [c,inn,n,cll,tx]])
                w.bind("<Leave>", lambda e, c=card, inn=inner, n=nl, cll=cl, tx=txt_f: [x.config(bg=SIDEBAR) for x in [c,inn,n,cll,tx]])
                # WE REMOVED THE DIRECT MOUSEWHEEL BIND HERE TO PREVENT MULTI-SCROLL BUGS

