extends Node

func _ready():
	var gv = ClassDB.instantiate("GameView")
	print("GameView: ", gv, " -> ", gv.hello())
	get_tree().quit()
