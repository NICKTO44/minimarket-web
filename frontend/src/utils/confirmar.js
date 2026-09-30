// Función para pedir confirmación con el diálogo propio del sistema
// (components/DialogoConfirmacion.jsx, montado una sola vez en main.jsx).
//
//   if (!(await confirmar({ titulo: '¿Eliminar...?', ... }))) return;
//
// Devuelve una promesa: true si confirmó; false si canceló, tocó fuera
// o presionó Escape.

let abrirDialogo = null;

/** Lo usa DialogoConfirmacion al montarse (y null al desmontarse). */
export function registrarDialogo(funcionAbrir) {
  abrirDialogo = funcionAbrir;
}

/**
 * @param {object} opciones
 * @param {string} opciones.titulo
 * @param {string} [opciones.mensaje]
 * @param {{ etiqueta: string, valor: string }} [opciones.detalle] recuadro con un dato importante
 * @param {string} [opciones.textoConfirmar='Confirmar']
 * @param {string} [opciones.textoCancelar='Cancelar']
 * @param {'peligro'|'normal'} [opciones.tipo='peligro'] peligro = botón rojo; normal = color del negocio
 * @param {'eliminar'|'aviso'|'tiempo'|'usuario'|'reactivar'} [opciones.icono]
 * @returns {Promise<boolean>}
 */
export function confirmar(opciones) {
  if (!abrirDialogo) {
    // Respaldo por si el diálogo no está montado: nunca ejecutar la
    // acción sin preguntar.
    return Promise.resolve(window.confirm(opciones.titulo));
  }
  return abrirDialogo(opciones);
}
