extends Node
## Positional game audio (filled in once the sound bank exists).
func play_shot(_dmg: int, _pos: Vector3) -> void: pass
func play_explosion(_size: float, _pos: Vector3) -> void: pass
func play_splash(_pos: Vector3) -> void: pass
func on_event(_e: Dictionary) -> void: pass
