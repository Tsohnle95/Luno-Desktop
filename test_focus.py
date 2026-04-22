import tkinter as tk
root = tk.Tk()
e = tk.Entry(root)
e.pack()
ph = "Filter tracks…"
e.insert(0, ph)
def fin(e_obj):
    if e.get() == ph:
        e.delete(0, "end")
        print("cleared")
def fout(e_obj):
    if not e.get():
        e.insert(0, ph)
        print("restored")
e.bind("<FocusIn>", fin)
e.bind("<FocusOut>", fout)
root.after(1000, lambda: root.destroy())
root.mainloop()
