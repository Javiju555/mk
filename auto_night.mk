# Script de automatización nocturna
# Ejecutar con: mk run auto_night.mk

# 2:34 AM - Pegar mensaje en la ventana activa
at 02:34 {
  paste "sigue entonces, sin mi... todo lo que puedas"
  enter
}

# 3:43 AM - Mover ratón a tercera pantalla, click para foco, pegar "sigue"
at 03:43 {
  move 4000 500
  wait 300ms
  click 4000 500
  wait 500ms
  paste "sigue"
  enter
}
