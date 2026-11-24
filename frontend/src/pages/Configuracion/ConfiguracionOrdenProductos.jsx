import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import './ConfiguracionOrdenProductos.css';

/**
 * Configuración → Orden de la lista de productos (migración 0072).
 * Desmarcada (por defecto): por nombre, como siempre. Marcada: la lista de
 * Productos y los Excel que se descargan salen por código interno, de menor
 * a mayor. Queda guardado para todo el negocio. Solo el administrador.
 */
export default function ConfiguracionOrdenProductos({ onMensaje }) {
  const [porCodigo, setPorCodigo] = useState(null);
  const [guardando, setGuardando] = useState(false);

  useEffect(() => {
    api
      .ordenProductos()
      .then((o) => setPorCodigo(!!o.por_codigo))
      .catch(() => setPorCodigo(null));
  }, []);

  if (porCodigo === null) return null;

  const cambiar = async (valor) => {
    setGuardando(true);
    onMensaje(null);
    try {
      const o = await api.ordenProductosGuardar(valor);
      setPorCodigo(!!o.por_codigo);
      onMensaje({
        tipo: 'exito',
        texto: o.por_codigo
          ? 'Listo: la lista de productos y los Excel salen ordenados por código interno.'
          : 'Listo: la lista de productos vuelve a ordenarse por nombre.',
      });
    } catch (e) {
      onMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  return (
    <div className="cfg-card">
      <h3 className="cfg-subtitulo-seccion">Lista de productos</h3>
      <label className="cfo-check">
        <input type="checkbox" checked={porCodigo} disabled={guardando} onChange={(e) => cambiar(e.target.checked)} />
        Ordenar productos por código interno
      </label>
      <p className="cfg-nota-moneda">
        Si la marcas, la lista de Productos y los Excel que descargues salen por código, de menor a mayor (9001, 19001,
        25012, 25012-S…). Si no, se ordenan por nombre. Vale para todo el negocio, en cualquier computadora.
      </p>
    </div>
  );
}
