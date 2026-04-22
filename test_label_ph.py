import tkinter as tk
root = tk.Tk()
sf = tk.Frame(root, bg="white", padx=10, pady=10)
sf.pack()
tk.Label(sf, text="Q", bg="white").pack(side="left")
entry = tk.Entry(sf, bg="white", bd=0, highlightthickness=0)
entry.pack(side="left", fill="x", expand=True)

ph_lbl = tk.Label(sf, text="Filter tracks...", bg="white", fg="gray", cursor="text")
ph_lbl.place(x=25, y=10) # 25 is after Q label approximately

def on_click(e):
    entry.focus_set()
ph_lbl.bind("<Button-1>", on_click)

def on_type(e):
    if entry.get():
        ph_lbl.place_forget()
    else:
        ph_lbl.place(x=25, y=10)
entry.bind("<KeyRelease>", on_type)

root.after(1000, lambda: root.destroy())
root.mainloop()
