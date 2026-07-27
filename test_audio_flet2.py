import flet as ft
import flet_audio as fta

def main(page: ft.Page):
    def on_state(e):
        print(f"State: {e.state}")
    
    def on_duration(e):
        print(f"Duration: {e.duration}")
        
    def on_loaded(e):
        print("Loaded!")
        
    def on_pos(e):
        print(f"Pos: {e.position}")

    audio = fta.Audio(
        src="file:///Users/ty/Music/app_music_folder/Synthwave/'Back To The 80's' ｜ Best of Synthwave And Retro Electro Music Mix ｜ Vol. 22.mp3",
        autoplay=True,
        on_state_change=on_state,
        on_duration_change=on_duration,
        on_loaded=on_loaded,
        on_position_change=on_pos
    )
    page.overlay.append(audio)
    page.update()

ft.run(main)
