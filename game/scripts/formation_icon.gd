extends Control
## Small drawn icon for a formation shape or arrival timing (formation bar buttons).
## kind "shape": 0 block, 1 line, 2 wedge, 3 column, 4 wide spread
## kind "timing": 0 free, 1 together, 2 next wave

var kind := "shape"
var index := 0
var color := Color(0.92, 0.88, 0.78)

func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE

func _dot(p: Vector2, r := 2.6) -> void:
	draw_circle(p, r, color)

func _draw() -> void:
	var c := size / 2.0
	var s := minf(size.x, size.y) / 2.0 - 3.0
	if kind == "shape":
		# the arrow shows the direction of travel
		draw_line(Vector2(c.x, c.y - s), Vector2(c.x, c.y - s + 5), Color(color, 0.5), 1.5)
		match index:
			0:
				for i in 3:
					for j in 3:
						_dot(c + Vector2((i - 1) * s * 0.5, (j - 0.5) * s * 0.45))
			1:
				for i in 5:
					_dot(c + Vector2((i - 2) * s * 0.42, s * 0.2))
			2:
				_dot(c + Vector2(0, -s * 0.35))
				for k in [1, 2]:
					_dot(c + Vector2(-k * s * 0.38, -s * 0.35 + k * s * 0.38))
					_dot(c + Vector2(k * s * 0.38, -s * 0.35 + k * s * 0.38))
			3:
				for j in 5:
					_dot(c + Vector2(0, (j - 1.6) * s * 0.36))
			4:
				for i in 5:
					_dot(c + Vector2((i - 2) * s * 0.48, s * 0.25), 2.0)
				draw_line(c + Vector2(-s, s * 0.25), c + Vector2(-s * 0.85, s * 0.25), color, 1.0)
				draw_line(c + Vector2(s * 0.85, s * 0.25), c + Vector2(s, s * 0.25), color, 1.0)
	else:
		# a finish line on the right; dots show where each unit is when the first arrives
		var fx := c.x + s * 0.75
		draw_line(Vector2(fx, c.y - s * 0.8), Vector2(fx, c.y + s * 0.8), color, 2.0)
		match index:
			0:
				for k in 3:
					_dot(Vector2(fx - 3 - k * s * 0.55, c.y + (k - 1) * s * 0.5))
			1:
				for k in 3:
					_dot(Vector2(fx - 4, c.y + (k - 1) * s * 0.5))
			2:
				for k in 3:
					_dot(Vector2(fx - 4, c.y + (k - 1) * s * 0.5), 2.0)
					_dot(Vector2(fx - 4 - s * 0.7, c.y + (k - 1) * s * 0.5), 2.0)
