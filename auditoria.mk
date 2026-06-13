# Esperar hasta las 4:33 AM (hora local)
at "04:33" {
    # Copiar y pegar el primer mensaje
    paste "continua, modo auditoría: on. ya sabes que esto pasa cuando me voy a dormir y esto es un mensaje mandado por mk, así que ya me dirás qué encuentras nuevo tras la auditoria de anoche, tui, server, common, core... ya sabes que hay que auditar por las noches tu solo todo, lo de arreglar se hace por el dia... con la misiónd e tener todo repasado por código y luego pruebas mías. Buenas noches. mañana te escribo"
    enter

    # Esperar 5 horas
    wait "5h"

    # Copiar y pegar el segundo mensaje
    paste "responde solo ok para iniciar el uso de una sesion de 5h la cual pillaré a mitad. responde solo ok"
    enter
}
