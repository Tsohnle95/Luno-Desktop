import tkinter as tk
from theme import BG, BG2, BG3, TEXT, TEXT2, TEXT3, SIDEBAR, BORDER

class ErrorLogView(tk.Frame):
    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller
        self._build_ui()

    def _build_ui(self):
        # Header
        hdr = tk.Frame(self, bg=BG, height=80)
        hdr.pack(fill="x", padx=32, pady=(40,20))
        tk.Label(hdr, text="Error & Debug Logs", font=("Helvetica Neue", 24, "bold"),
                 fg=TEXT, bg=BG).pack(side="left")
        
        clr = tk.Label(hdr, text="Clear Logs", font=("Helvetica Neue", 11),
                        fg=TEXT3, bg=BG, cursor="hand2")
        clr.pack(side="right", pady=(10,0))
        clr.bind("<Button-1>", lambda e: self._clear_logs())
        clr.bind("<Enter>", lambda e: clr.config(fg=TEXT))
        clr.bind("<Leave>", lambda e: clr.config(fg=TEXT3))

        # Main Text Area
        self.container = tk.Frame(self, bg=BG2, bd=1, highlightbackground=BORDER, highlightthickness=1)
        self.container.pack(fill="both", expand=True, padx=32, pady=(0,40))

        self.text = tk.Text(self.container, bg=BG2, fg=TEXT2, font=("Menlo", 11),
                             relief="flat", bd=0, highlightthickness=0,
                             padx=15, pady=15, state="disabled", wrap="word")
        self.text.pack(side="left", fill="both", expand=True)

        from widgets import ModernScrollbar
        self.sb = ModernScrollbar(self.container, command=self.text.yview)
        self.text.config(yscrollcommand=self.sb.set)
        self.sb.pack(side="right", fill="y")

    def _clear_logs(self):
        self.controller._error_logs = []
        self.populate()

    def populate(self):
        self.text.config(state="normal")
        self.text.delete("1.0", "end")
        if not self.controller._error_logs:
            self.text.insert("end", "--- No logs yet ---")
        else:
            for log in self.controller._error_logs:
                self.text.insert("end", log + "\n")
        self.text.config(state="disabled")
        self.text.see("end")
