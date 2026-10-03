import { useState } from 'react';
import { api } from '../../api/api';
import { AFECTACIONES } from '../../utils/igv';

/**
 * Categorías del negocio y su IGV. Marcar una categoría como exonerada
 * hace que todos sus productos (y los que se agreguen después) lo hereden,
 * salvo los que tengan un valor propio. Solo el administrador lo cambia.
 */
export default function CategoriasIgv({ categorias, esAdmin, onCerrar, onCambiado }) {
  const [guardando, setGuardando] = useState(null);
  const [mensaje, setMensaje] = useState(null);

  const cambiar = async (categoria, valor) => {
    setGuardando(categoria.id);
    setMensaje(null);
    try {
      const r = await api.categoriaIgv(categoria.id, valor);
      onCambiado();
      setMensaje({
        tipo: 'exito',
        texto: `"${categoria.nombre}" ahora es ${r.afectacion_igv.toLowerCase()}. Lo ${r.productos === 1 ? 'hereda 1 producto' : `heredan ${r.productos} productos`}.`,
      });
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(null);
    }
  };

  return (
    <div className="inv-modal-overlay" onClick={onCerrar}>
      <div className="inv-modal inv-modal-categorias" onClick={(e) => e.stopPropagation()}>
        <h2>Categorías e IGV</h2>
        <p className="inv-cat-nota">
          Marca como <strong>Exonerado</strong> la categoría completa (por ejemplo Frutas o Verduras). Todos sus
          productos, y los que agregues después, lo heredan. Si un producto es una excepción, se cambia en el propio
          producto. Qué productos son exonerados lo define la norma: confírmalo con tu contador.
        </p>
        {mensaje && <p className={`inv-mensaje inv-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}
        {categorias.length === 0 ? (
          <p className="inv-cat-nota">Aún no hay categorías. Se crean al agregar un producto.</p>
        ) : (
          <table className="inv-cat-tabla">
            <thead>
              <tr>
                <th>Categoría</th>
                <th>IGV</th>
              </tr>
            </thead>
            <tbody>
              {categorias.map((c) => (
                <tr key={c.id}>
                  <td>{c.nombre}</td>
                  <td>
                    <select
                      className={c.afectacion_igv && c.afectacion_igv !== 'GRAVADO' ? 'inv-cat-sin-igv' : ''}
                      value={c.afectacion_igv || 'GRAVADO'}
                      disabled={!esAdmin || guardando === c.id}
                      onChange={(e) => cambiar(c, e.target.value)}
                      aria-label={`IGV de ${c.nombre}`}
                    >
                      {AFECTACIONES.map((a) => (
                        <option key={a.valor} value={a.valor}>
                          {a.label}
                        </option>
                      ))}
                    </select>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {!esAdmin && <p className="inv-cat-nota">Solo el administrador puede cambiar el IGV.</p>}
        <div className="inv-modal-acciones">
          <button className="inv-boton-cancelar" onClick={onCerrar}>
            Cerrar
          </button>
        </div>
      </div>
    </div>
  );
}
