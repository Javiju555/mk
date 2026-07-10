# Script de prueba - ejecutar con: mk run test_auto.mk
# Prueba 1: en 2 minutos
at 00:50 {
  paste "testeando automación 1"
  enter
}

# Prueba 2: en 4 minutos - con click para foco
at 00:52 {
  move 4000 500
  wait 300ms
  click 4000 500
  wait 500ms
  paste "testeando automación 2"
  enter
}
