import { useState, useEffect, useMemo } from 'react';
import { api, API_URL } from '../../api/api';
import './Inventario.css';
import EscanerCodigoBarras from '../../components/EscanerCodigoBarras';
import { confirmar } from '../../utils/confirmar';
import { comprimirImagen } from '../../utils/comprimirImagen';
import { etiquetaUnidad, opcionesUnidad } from '../../utils/unidades';
import { AFECTACIONES, etiquetaAfectacion } from '../../utils/igv';
import CategoriasIgv from './CategoriasIgv';
import FormularioModelo from './FormularioModelo';
import ImportarProductos from './ImportarProductos';
import { TIPO_XLSX, escribirXlsx } from '../../utils/hojas';
import { descargarArchivo } from '../../utils/pdfTicket';
import { hoyLima } from '../../utils/formato';
import { agruparPorModelo, etiquetaVariante, rangoPrecio } from '../../utils/variantes';

const FORM_VACIO = {
  codigo: '',
  nombre: '',
  descripcion: '',
  precio: '',
  stock: '',
  stock_minimo: '5',
  unidad_medida: 'UNIDAD',
  categoria_id: '',
  lleva_vencimiento: false,
  precio_compra: '',
  controla_stock: true,
  // 'HEREDAR' = el IGV de su categoría; o GRAVADO / EXONERADO / INAFECTO.
  afectacion_igv: 'HEREDAR',
};

// Stock bajo solo aplica a productos que controlan stock (un café
// preparado al momento no tiene stock que reponer).
const tieneStockBajo = (p) => p.controla_stock !== false && p.stock <= p.stock_minimo;
// Para el reporte de ganancias: productos con stock que no dicen cuánto costaron.
const sinPrecioCompra = (p) => p.controla_stock !== false && !(p.precio_compra > 0);

// Un modelo (módulo "Tallas y colores"): una fila con el resumen y, al
// desplegarla, una fila por cada talla y color con su código, precio y stock.
function FilasModelo({ grupo, abierto, onAlternar, onEditar }) {
  return (
    <>
      <tr className="inv-fila-modelo">
        <td>
          {grupo.imagen_url ? (
            <img className="inv-miniatura" src={`${API_URL}${grupo.imagen_url}`} alt={grupo.nombre} />
          ) : (
            <div className="inv-miniatura inv-miniatura-vacia">👕</div>
          )}
        </td>
        <td>{grupo.variantes.length === 1 ? grupo.variantes[0].codigo : `${grupo.variantes.length} códigos`}</td>
        <td>
          <span className="inv-modelo-nombre">{grupo.nombre}</span>
          <div className="inv-modelo-tallas">
            {grupo.variantes.map((v) => (
              <span key={v.id} className={`inv-modelo-talla${v.stock <= 0 ? ' agotada' : ''}`} title={`Stock: ${v.stock}`}>
                {etiquetaVariante(v)}
              </span>
            ))}
          </div>
          <button type="button" className="inv-modelo-ver" onClick={onAlternar} aria-expanded={abierto}>
            {abierto ? 'Ocultar tallas ▴' : 'Ver precio y stock por talla ▾'}
          </button>
        </td>
        <td>{grupo.categoria_nombre || '—'}</td>
        <td>{rangoPrecio(grupo)}</td>
        <td className={grupo.conStockBajo > 0 ? 'inv-stock-bajo' : ''}>
          {grupo.stockTotal} {grupo.conStockBajo > 0 && '⚠'}
        </td>
        <td>{etiquetaUnidad(grupo.unidad_medida)}</td>
        <td>
          <button className="inv-boton-editar" onClick={onEditar}>
            Editar
          </button>
        </td>
      </tr>
      {abierto &&
        grupo.variantes.map((v) => (
          <tr key={v.id} className={`inv-fila-variante${tieneStockBajo(v) ? ' inv-fila-alerta' : ''}`}>
            <td />
            <td>{v.codigo}</td>
            <td>{etiquetaVariante(v)}</td>
            <td />
            <td>S/ {v.precio.toFixed(2)}</td>
            <td className={tieneStockBajo(v) ? 'inv-stock-bajo' : ''}>
              {v.stock} {tieneStockBajo(v) && '⚠'}
            </td>
            <td />
            <td />
          </tr>
        ))}
    </>
  );
}

// Las unidades que se ofrecen son las que el negocio activó en
// Configuración → Unidades (el catálogo completo está en utils/unidades.js).

// servicios: el negocio tiene el módulo "Servicios y venta sin stock".
// etiquetas: nombres propios del rubro ("Carta" en un restaurante).
// esAdmin: solo el administrador cambia el IGV de categorías y productos.
// variantes: el negocio tiene el módulo "Tallas y colores" (ropa y calzado):
// los productos de un mismo modelo se muestran juntos y se crean de una vez.
// Apagado, esta pantalla es la de siempre.
// ganancias: el negocio tiene el módulo "Reporte de ganancias": el precio de
// compra pasa a ser obligatorio y se muestra el costo promedio. Apagado, el
// precio de compra sigue siendo opcional, como siempre.
// verSinPrecio: abrir ya filtrado por "sin precio de compra" (desde Reportes).
export default function Inventario({
  servicios = false,
  etiquetas = {},
  esAdmin = false,
  variantes = false,
  ganancias = false,
  verSinPrecio = false,
  onSinPrecioVisto,
}) {
  const [productos, setProductos] = useState([]);
  const [categorias, setCategorias] = useState([]);
  const [busqueda, setBusqueda] = useState('');
  const [filtroCategoria, setFiltroCategoria] = useState('');
  const [soloStockBajo, setSoloStockBajo] = useState(false);
  const [soloSinPrecio, setSoloSinPrecio] = useState(ganancias && verSinPrecio);
  // Costo promedio de cada producto (módulo de ganancias): { id: costo }
  const [costos, setCostos] = useState({});
  // Productos desactivados ("archivados" porque ya tenían ventas o compras):
  // se ven aparte y se pueden reactivar.
  const [desactivados, setDesactivados] = useState([]);
  // Unidades activas del negocio (null = aún no llegan: se usan las de siempre).
  const [unidadesActivas, setUnidadesActivas] = useState(null);
  const [verCategorias, setVerCategorias] = useState(false);
  const [verDesactivados, setVerDesactivados] = useState(false);
  const [cargando, setCargando] = useState(true);

  const [mostrarForm, setMostrarForm] = useState(false);
  const [editandoId, setEditandoId] = useState(null);
  const [form, setForm] = useState(FORM_VACIO);
  const [guardando, setGuardando] = useState(false);
  const [mensaje, setMensaje] = useState(null);

  const [imagenArchivo, setImagenArchivo] = useState(null);
  const [imagenPreview, setImagenPreview] = useState(null);

  const [loteInicialCantidad, setLoteInicialCantidad] = useState('');
  const [loteInicialFecha, setLoteInicialFecha] = useState('');

  const [lotesProducto, setLotesProducto] = useState([]);
  const [cargandoLotes, setCargandoLotes] = useState(false);
  const [nuevoLoteCantidad, setNuevoLoteCantidad] = useState('');
  const [nuevoLoteFecha, setNuevoLoteFecha] = useState('');
  const [agregandoLote, setAgregandoLote] = useState(false);

  // --- Crear categoría nueva, sin salir del formulario de producto ---
  const [mostrarNuevaCategoria, setMostrarNuevaCategoria] = useState(false);
  const [nombreNuevaCategoria, setNombreNuevaCategoria] = useState('');
  const [creandoCategoria, setCreandoCategoria] = useState(false);

  // --- Escáner de código de barras (modo una sola lectura) ---
  const [escanerCodigoAbierto, setEscanerCodigoAbierto] = useState(false);

  // --- Tallas y colores: formulario del modelo y modelos desplegados ---
  const [modeloForm, setModeloForm] = useState(null);
  // Importar productos desde Excel o CSV (solo el administrador).
  const [importando, setImportando] = useState(false);
  const [modelosAbiertos, setModelosAbiertos] = useState(() => new Set());

  const cargarTodo = () => {
    setCargando(true);
    Promise.all([api.productos(), api.categorias()])
      .then(([p, c]) => {
        // Los platos de la carta del día se manejan en "Carta de hoy".
        setProductos(p.filter((x) => !x.carta_dia));
        setCategorias(c);
      })
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargando(false));
    // Aparte y sin bloquear: si fallara, el inventario igual se muestra.
    api.productosDesactivados().then(setDesactivados).catch(() => setDesactivados([]));
    api.unidades().then((u) => setUnidadesActivas(u.activas)).catch(() => {});
    if (ganancias) {
      api
        .gananciasCostos()
        .then((lista) => setCostos(Object.fromEntries(lista.map((c) => [c.producto_id, c.costo_promedio]))))
        .catch(() => {});
    }
  };

  useEffect(() => {
    cargarTodo();
    // El pedido de abrir filtrado ya se atendió: la próxima vez se abre normal.
    if (verSinPrecio) onSinPrecioVisto?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const productosFiltrados = useMemo(() => {
    let lista = verDesactivados ? desactivados : productos;
    if (busqueda.trim()) {
      const q = busqueda.toLowerCase();
      lista = lista.filter((p) => p.nombre.toLowerCase().includes(q) || p.codigo.includes(q));
    }
    if (filtroCategoria) {
      lista = lista.filter((p) => String(p.categoria_id) === filtroCategoria);
    }
    if (soloStockBajo && !verDesactivados) {
      lista = lista.filter(tieneStockBajo);
    }
    if (soloSinPrecio && !verDesactivados) {
      lista = lista.filter(sinPrecioCompra);
    }
    return lista;
  }, [productos, desactivados, verDesactivados, busqueda, filtroCategoria, soloStockBajo, soloSinPrecio]);

  // Con tallas y colores, las tallas de un modelo van en una sola fila.
  const filasTabla = useMemo(
    () => (variantes && !verDesactivados ? agruparPorModelo(productosFiltrados) : productosFiltrados),
    [variantes, verDesactivados, productosFiltrados]
  );
  // El modelo completo (aunque el buscador solo muestre algunas tallas).
  const modeloCompleto = (modeloId) => agruparPorModelo(productos).find((g) => g.esModelo && g.modelo_id === modeloId);

  const alternarModelo = (modeloId) => {
    setModelosAbiertos((prev) => {
      const siguiente = new Set(prev);
      if (siguiente.has(modeloId)) siguiente.delete(modeloId);
      else siguiente.add(modeloId);
      return siguiente;
    });
  };

  const reactivarProducto = async (p) => {
    const confirmado = await confirmar({
      titulo: `¿Reactivar "${p.nombre}"?`,
      mensaje: 'Volverá a aparecer en el POS y en el inventario con su mismo código, precio, stock e historial.',
      textoConfirmar: 'Reactivar',
      tipo: 'normal',
      icono: 'reactivar',
    });
    if (!confirmado) return;
    try {
      await api.productoReactivar(p.id);
      setMensaje({ tipo: 'exito', texto: `"${p.nombre}" reactivado. Ya aparece en el POS.` });
      cargarTodo();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

  const abrirNuevo = () => {
    setEditandoId(null);
    setForm(FORM_VACIO);
    setLoteInicialCantidad('');
    setLoteInicialFecha('');
    setLotesProducto([]);
    setImagenArchivo(null);
    setImagenPreview(null);
    setMensaje(null);
    setMostrarNuevaCategoria(false);
    setNombreNuevaCategoria('');
    setMostrarForm(true);
  };

  const abrirEdicion = (p) => {
    setEditandoId(p.id);
    setForm({
      codigo: p.codigo,
      nombre: p.nombre,
      descripcion: p.descripcion || '',
      precio: String(p.precio),
      stock: String(p.stock),
      stock_minimo: String(p.stock_minimo),
      unidad_medida: p.unidad_medida,
      categoria_id: String(p.categoria_id),
      lleva_vencimiento: p.lleva_vencimiento,
      precio_compra: String(p.precio_compra || ''),
      controla_stock: p.controla_stock !== false,
      afectacion_igv: p.afectacion_propia || 'HEREDAR',
    });
    setImagenArchivo(null);
    setImagenPreview(p.imagen_url ? `${API_URL}${p.imagen_url}?t=${Date.now()}` : null);
    setMensaje(null);
    setMostrarNuevaCategoria(false);
    setNombreNuevaCategoria('');
    setMostrarForm(true);

    if (p.lleva_vencimiento) {
      cargarLotes(p.id);
    } else {
      setLotesProducto([]);
    }
  };

  const cargarLotes = (productoId) => {
    setCargandoLotes(true);
    api
      .lotesDeProducto(productoId)
      .then(setLotesProducto)
      .catch(() => setLotesProducto([]))
      .finally(() => setCargandoLotes(false));
  };

  const cerrarForm = () => {
    setMostrarForm(false);
    setEditandoId(null);
    setForm(FORM_VACIO);
    setLotesProducto([]);
    setLoteInicialCantidad('');
    setLoteInicialFecha('');
    setImagenArchivo(null);
    setImagenPreview(null);
    setMostrarNuevaCategoria(false);
    setNombreNuevaCategoria('');
  };

  const cambiarCampo = (campo, valor) => {
    setForm((f) => ({ ...f, [campo]: valor }));
  };

  const crearCategoriaNueva = async () => {
    if (!nombreNuevaCategoria.trim()) {
      setMensaje({ tipo: 'error', texto: 'Escribe un nombre para la categoría.' });
      return;
    }
    setCreandoCategoria(true);
    try {
      const nueva = await api.categoriaCrear({ nombre: nombreNuevaCategoria.trim() });
      const categoriasActualizadas = await api.categorias();
      setCategorias(categoriasActualizadas);
      cambiarCampo('categoria_id', String(nueva.id));
      setMostrarNuevaCategoria(false);
      setNombreNuevaCategoria('');
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setCreandoCategoria(false);
    }
  };

  const manejarSeleccionImagen = (e) => {
    const archivo = e.target.files?.[0];
    if (!archivo) return;
    if (!archivo.type.startsWith('image/')) {
      setMensaje({ tipo: 'error', texto: 'El archivo debe ser una imagen.' });
      return;
    }
    setImagenArchivo(archivo);
    setImagenPreview(URL.createObjectURL(archivo));
  };

  const agregarLoteAProductoExistente = async () => {
    if (!nuevoLoteCantidad || !nuevoLoteFecha) {
      setMensaje({ tipo: 'error', texto: 'Completa cantidad y fecha de vencimiento del lote.' });
      return;
    }
    setAgregandoLote(true);
    try {
      await api.loteCrear({
        producto_id: editandoId,
        cantidad: parseFloat(nuevoLoteCantidad),
        fecha_vencimiento: nuevoLoteFecha,
      });
      setNuevoLoteCantidad('');
      setNuevoLoteFecha('');
      cargarLotes(editandoId);
      cargarTodo();
      setMensaje({ tipo: 'exito', texto: 'Lote agregado.' });
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setAgregandoLote(false);
    }
  };

  const validarYGuardar = async () => {
    setMensaje(null);
    if (!form.codigo.trim() || !form.nombre.trim() || !form.categoria_id) {
      setMensaje({ tipo: 'error', texto: 'Código, nombre y categoría son obligatorios.' });
      return;
    }
    const precio = parseFloat(form.precio);
    if (isNaN(precio) || precio <= 0) {
      setMensaje({ tipo: 'error', texto: 'El precio debe ser mayor a 0.' });
      return;
    }

    // Preparado al momento (cafetería): no pide stock.
    const sinControlStock = servicios && !form.controla_stock;

    // Reporte de ganancias encendido: el precio de compra es obligatorio.
    if (ganancias && !sinControlStock && !(parseFloat(form.precio_compra) > 0)) {
      setMensaje({
        tipo: 'error',
        texto: 'Falta el precio de compra: con el reporte de ganancias activo es obligatorio (lo que te cuesta, con IGV).',
      });
      return;
    }

    if (sinControlStock) {
      // nada que validar
    } else if (!form.lleva_vencimiento) {
      const stock = parseFloat(form.stock);
      if (isNaN(stock) || stock < 0) {
        setMensaje({ tipo: 'error', texto: 'El stock no puede ser negativo.' });
        return;
      }
    } else if (!editandoId) {
      if (!loteInicialCantidad || !loteInicialFecha) {
        setMensaje({
          tipo: 'error',
          texto: 'Como es perecible, indica la cantidad y fecha de vencimiento del primer lote.',
        });
        return;
      }
    }

    const payload = {
      codigo: form.codigo.trim(),
      nombre: form.nombre.trim(),
      descripcion: form.descripcion.trim() || null,
      precio,
      stock: form.lleva_vencimiento ? 0 : sinControlStock ? parseFloat(form.stock) || 0 : parseFloat(form.stock),
      stock_minimo: parseFloat(form.stock_minimo) || 0,
      unidad_medida: form.unidad_medida,
      categoria_id: parseInt(form.categoria_id, 10),
      lleva_vencimiento: sinControlStock ? false : form.lleva_vencimiento,
      precio_compra: form.precio_compra ? parseFloat(form.precio_compra) : 0,
      // Solo se manda con el módulo de servicios; sin él el backend
      // deja el valor que ya tenía (controla stock, como siempre).
      ...(servicios ? { controla_stock: form.controla_stock } : {}),
    };

    setGuardando(true);
    try {
      let idParaImagen = editandoId;

      if (editandoId) {
        await api.productoActualizar(editandoId, payload);
      } else {
        const creado = await api.productoCrear(payload);
        idParaImagen = creado.producto_id;
        if (form.lleva_vencimiento && !sinControlStock && creado.producto_id) {
          await api.loteCrear({
            producto_id: creado.producto_id,
            cantidad: parseFloat(loteInicialCantidad),
            fecha_vencimiento: loteInicialFecha,
          });
        }
      }

      // IGV propio del producto (solo el administrador, y solo si cambió).
      if (esAdmin && idParaImagen) {
        const anterior = editandoId ? productos.find((x) => x.id === editandoId)?.afectacion_propia || 'HEREDAR' : 'HEREDAR';
        if (form.afectacion_igv !== anterior) {
          try {
            await api.productoIgv(idParaImagen, form.afectacion_igv);
          } catch (e) {
            setEditandoId(idParaImagen);
            setMensaje({ tipo: 'error', texto: `Producto guardado, pero no se pudo cambiar su IGV (${e.message}).` });
            cargarTodo();
            return;
          }
        }
      }

      if (imagenArchivo && idParaImagen) {
        try {
          await api.productoSubirImagen(idParaImagen, await comprimirImagen(imagenArchivo));
        } catch (e) {
          // El producto YA existe: el formulario pasa a "editar" ese mismo
          // producto, así "Guardar" de nuevo solo reintenta la foto y no
          // crea un producto repetido.
          setEditandoId(idParaImagen);
          setMensaje({
            tipo: 'error',
            texto: `Producto guardado, pero la foto no se pudo subir (${e.message}). Toca Guardar para intentarlo de nuevo.`,
          });
          cargarTodo();
          return;
        }
      }

      setMensaje({ tipo: 'exito', texto: editandoId ? 'Producto actualizado.' : 'Producto agregado.' });
      cerrarForm();
      cargarTodo();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  const eliminarOProducto = async (p) => {
    const confirmado = await confirmar({
      titulo: `¿Eliminar "${p.nombre}"?`,
      mensaje: 'Se borrará del inventario. Esta acción no se puede deshacer.',
      textoConfirmar: 'Eliminar',
      icono: 'eliminar',
    });
    if (!confirmado) return false;
    try {
      const resultado = await api.productoEliminar(p.id);
      if (!resultado.success) {
        const desactivar = await confirmar({
          titulo: 'No se puede eliminar',
          mensaje: `"${p.nombre}" ya tiene ventas o compras registradas, por eso no se puede borrar. Si lo desactivas, dejará de aparecer en el POS, pero su historial se conserva.`,
          textoConfirmar: 'Desactivar',
          tipo: 'normal',
          icono: 'aviso',
        });
        if (desactivar) {
          await api.productoDesactivar(p.id);
          setMensaje({ tipo: 'exito', texto: 'Producto desactivado.' });
          cargarTodo();
          return true;
        }
        return false;
      }
      setMensaje({ tipo: 'exito', texto: 'Producto eliminado.' });
      cargarTodo();
      return true;
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
      return false;
    }
  };

  // Excel con todos los productos activos. Sirve también de plantilla: se
  // edita y se vuelve a importar (los códigos que ya existen se actualizan).
  const exportarProductos = () => {
    const conTallas = productos.some((p) => p.modelo_id != null);
    const titulos = ['Código', 'Nombre', 'Descripción', 'Categoría', 'Unidad', 'Precio de venta', 'Precio de compra', 'Stock', 'Stock mínimo'];
    if (conTallas) titulos.push('Modelo', 'Talla', 'Color');
    const filas = productos.map((p) => {
      const fila = [
        String(p.codigo),
        p.nombre,
        p.descripcion || '',
        p.categoria_nombre || '',
        etiquetaUnidad(p.unidad_medida),
        p.precio,
        p.precio_compra > 0 ? p.precio_compra : '',
        p.controla_stock === false ? '' : p.stock,
        p.stock_minimo,
      ];
      if (conTallas) fila.push(p.modelo_nombre || '', p.talla || '', p.color || '');
      return fila;
    });
    descargarArchivo(escribirXlsx([titulos, ...filas], { anchos: [16, 38, 30, 18, 12, 15, 16, 10, 13, 24, 10, 14] }), `Productos-${hoyLima()}.xlsx`, {
      tipo: TIPO_XLSX,
    });
  };

  const stockBajoCantidad = productos.filter(tieneStockBajo).length;
  const sinPrecioCantidad = ganancias ? productos.filter(sinPrecioCompra).length : 0;

  // Reporte de ganancias: qué pasará con el costo del producto al guardar.
  const ayudaCosto = (() => {
    if (!ganancias || !mostrarForm) return null;
    if (servicios && !form.controla_stock) return { texto: 'En un servicio o producto sin stock el precio de compra es opcional.' };
    const precio = parseFloat(form.precio_compra);
    if (!(precio > 0)) {
      return { falta: true, texto: 'Obligatorio: lo que te cuesta cada unidad, con IGV. Sin él no se puede calcular tu ganancia.' };
    }
    const actual = editandoId ? productos.find((p) => p.id === editandoId) : null;
    if (!actual) {
      return { texto: form.lleva_vencimiento ? 'Los lotes que agregues entran a este precio.' : 'El stock inicial entra a este precio.' };
    }
    const costoHoy = costos[actual.id] ?? (actual.precio_compra > 0 ? actual.precio_compra : null);
    const stockNuevo = parseFloat(form.stock);
    if (!form.lleva_vencimiento && stockNuevo > actual.stock + 1e-9) {
      const entran = stockNuevo - actual.stock;
      const habia = Math.max(actual.stock, 0);
      const promedio = habia > 0 && costoHoy > 0 ? (habia * costoHoy + entran * precio) / (habia + entran) : precio;
      return {
        texto: `Entran ${+entran.toFixed(3)} a S/ ${precio.toFixed(2)} cada una. Revisa que sea lo que pagaste: el costo promedio quedará en S/ ${promedio.toFixed(2)}.`,
      };
    }
    if (Math.abs(precio - (actual.precio_compra || 0)) > 0.005) {
      return {
        texto: `Cambias el precio de compra sin que entre mercadería: se toma como corrección y el costo pasa a S/ ${precio.toFixed(2)}.`,
      };
    }
    if (costoHoy && Math.abs(costoHoy - precio) > 0.005) {
      return { texto: `Costo promedio de lo que tienes en la tienda: S/ ${costoHoy.toFixed(2)}.` };
    }
    return null;
  })();

  return (
    <div className="inv-layout">
      <div className="inv-header">
        <h1>{etiquetas.tituloProductos || 'Inventario'}</h1>
        <div className="inv-header-acciones">
          {esAdmin && (
            <>
              <button
                className="inv-boton-categorias"
                onClick={() => {
                  setMensaje(null);
                  setImportando(true);
                }}
              >
                ⬆ Importar
              </button>
              <button className="inv-boton-categorias" onClick={exportarProductos} disabled={productos.length === 0}>
                ⬇ Exportar
              </button>
            </>
          )}
          <button className="inv-boton-categorias" onClick={() => setVerCategorias(true)}>
            Categorías e IGV
          </button>
          {variantes ? (
            <>
              <button className="inv-boton-modelo" onClick={abrirNuevo}>
                + Producto sin tallas
              </button>
              <button
                className="inv-boton-nuevo"
                onClick={() => {
                  setMensaje(null);
                  setModeloForm({ grupo: null });
                }}
              >
                + Nuevo modelo con tallas
              </button>
            </>
          ) : (
            <button className="inv-boton-nuevo" onClick={abrirNuevo}>
              {etiquetas.nuevoProducto || '+ Nuevo producto'}
            </button>
          )}
        </div>
      </div>

      <div className="inv-filtros">
        <input
          type="text"
          placeholder="Buscar por nombre o código..."
          value={busqueda}
          onChange={(e) => setBusqueda(e.target.value)}
        />
        <select value={filtroCategoria} onChange={(e) => setFiltroCategoria(e.target.value)}>
          <option value="">Todas las categorías</option>
          {categorias.map((c) => (
            <option key={c.id} value={c.id}>
              {c.nombre}
            </option>
          ))}
        </select>
        {!verDesactivados && (
          <button
            className={`inv-filtro-stock ${soloStockBajo ? 'activo' : ''}`}
            onClick={() => setSoloStockBajo((v) => !v)}
          >
            ⚠ Stock bajo {stockBajoCantidad > 0 && `(${stockBajoCantidad})`}
          </button>
        )}
        {!verDesactivados && (sinPrecioCantidad > 0 || soloSinPrecio) && (
          <button
            className={`inv-filtro-stock inv-filtro-sin-precio ${soloSinPrecio ? 'activo' : ''}`}
            onClick={() => setSoloSinPrecio((v) => !v)}
          >
            Sin precio de compra ({sinPrecioCantidad})
          </button>
        )}
        {(desactivados.length > 0 || verDesactivados) && (
          <button
            className={`inv-filtro-desactivados ${verDesactivados ? 'activo' : ''}`}
            onClick={() => setVerDesactivados((v) => !v)}
          >
            {verDesactivados ? '← Volver a activos' : `Desactivados (${desactivados.length})`}
          </button>
        )}
      </div>

      {verDesactivados && (
        <p className="inv-aviso-desactivados">
          Estos productos no aparecen en el POS. Se desactivaron porque ya tenían ventas o compras; su historial se
          conserva. Puedes reactivarlos cuando quieras.
        </p>
      )}

      {mensaje && !mostrarForm && (
        <p className={`inv-mensaje inv-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>
      )}

      {cargando ? (
        <p className="inv-cargando">Cargando...</p>
      ) : (
        <div className="inv-tabla-wrapper">
          <table className="inv-tabla">
            <thead>
              <tr>
                <th></th>
                <th>Código</th>
                <th>Nombre</th>
                <th>Categoría</th>
                <th>Precio</th>
                <th>Stock</th>
                <th>Unidad</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {filasTabla.map((p) =>
                p.esModelo ? (
                  <FilasModelo
                    key={p.id}
                    grupo={p}
                    abierto={modelosAbiertos.has(p.modelo_id)}
                    onAlternar={() => alternarModelo(p.modelo_id)}
                    onEditar={() => {
                      setMensaje(null);
                      setModeloForm({ grupo: modeloCompleto(p.modelo_id) || p });
                    }}
                  />
                ) : (
                  <tr
                    key={p.id}
                    className={
                      verDesactivados ? 'inv-fila-desactivada' : tieneStockBajo(p) ? 'inv-fila-alerta' : ''
                    }
                  >
                    <td>
                      {p.imagen_url ? (
                        <img className="inv-miniatura" src={`${API_URL}${p.imagen_url}`} alt={p.nombre} />
                      ) : (
                        <div className="inv-miniatura inv-miniatura-vacia">📦</div>
                      )}
                    </td>
                    <td>{p.codigo}</td>
                    <td>
                      {p.nombre}
                      {p.lleva_vencimiento && <span className="inv-badge-vencimiento">vence</span>}
                      {p.afectacion_igv && p.afectacion_igv !== 'GRAVADO' && (
                        <span className="inv-badge-sin-igv">{etiquetaAfectacion(p.afectacion_igv)}</span>
                      )}
                    </td>
                    <td>{p.categoria_nombre || '—'}</td>
                    <td>S/ {p.precio.toFixed(2)}</td>
                    <td className={!verDesactivados && tieneStockBajo(p) ? 'inv-stock-bajo' : ''}>
                      {p.controla_stock === false ? (
                        <span className="inv-badge-preparado">preparado</span>
                      ) : (
                        <>
                          {p.stock} {!verDesactivados && tieneStockBajo(p) && '⚠'}
                        </>
                      )}
                    </td>
                    <td>{etiquetaUnidad(p.unidad_medida)}</td>
                    <td>
                      {verDesactivados ? (
                        <button className="inv-boton-reactivar" onClick={() => reactivarProducto(p)}>
                          Reactivar
                        </button>
                      ) : (
                        <>
                          <button className="inv-boton-editar" onClick={() => abrirEdicion(p)}>
                            Editar
                          </button>
                          <button className="inv-boton-eliminar" onClick={() => eliminarOProducto(p)}>
                            Eliminar
                          </button>
                        </>
                      )}
                    </td>
                  </tr>
                )
              )}
              {filasTabla.length === 0 && (
                <tr>
                  <td colSpan={8} className="inv-sin-resultados">
                    {verDesactivados ? 'No hay productos desactivados.' : 'No hay productos que coincidan.'}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {importando && (
        <ImportarProductos
          categorias={categorias}
          unidadesActivas={unidadesActivas}
          onCerrar={() => setImportando(false)}
          onTerminado={(resultado) => {
            setImportando(false);
            setMensaje({
              tipo: 'exito',
              texto: `Importación terminada: ${resultado.creados} producto${resultado.creados === 1 ? '' : 's'} nuevo${resultado.creados === 1 ? '' : 's'}${
                resultado.actualizados > 0 ? ` y ${resultado.actualizados} actualizado${resultado.actualizados === 1 ? '' : 's'}` : ''
              }.`,
            });
            cargarTodo();
          }}
        />
      )}

      {verCategorias && (
        <CategoriasIgv
          categorias={categorias}
          esAdmin={esAdmin}
          onCerrar={() => setVerCategorias(false)}
          onCambiado={cargarTodo}
        />
      )}

      {mostrarForm && (
        <div className="inv-modal-overlay" onClick={cerrarForm}>
          <div className="inv-modal" onClick={(e) => e.stopPropagation()}>
            <h2>{editandoId ? 'Editar producto' : 'Nuevo producto'}</h2>

            <div className="inv-campo inv-campo-imagen">
              <label>Foto del producto (opcional)</label>
              <div className="inv-imagen-selector">
                {imagenPreview ? (
                  <img src={imagenPreview} alt="Vista previa" className="inv-imagen-preview" />
                ) : (
                  <div className="inv-imagen-preview inv-imagen-preview-vacia">📦</div>
                )}
                <label className="inv-boton-subir-imagen">
                  {imagenPreview ? 'Cambiar foto' : 'Elegir foto'}
                  <input type="file" accept="image/*" onChange={manejarSeleccionImagen} hidden />
                </label>
              </div>
              <p className="inv-imagen-nota">Se optimiza automáticamente al guardar (máx. 800px, comprimida).</p>
            </div>

            <div className="inv-form-grid">
              <div className="inv-campo">
                <label>Código</label>
                <div className="inv-campo-codigo-fila">
                  <input value={form.codigo} onChange={(e) => cambiarCampo('codigo', e.target.value)} />
                  <button
                    type="button"
                    className="inv-boton-escanear"
                    onClick={() => {
                      setMensaje(null);
                      setEscanerCodigoAbierto(true);
                    }}
                    aria-label="Escanear código de barras"
                  >
                    📷
                  </button>
                </div>
              </div>
              <div className="inv-campo">
                <label>Nombre</label>
                <input value={form.nombre} onChange={(e) => cambiarCampo('nombre', e.target.value)} />
              </div>
              <div className="inv-campo inv-campo-full">
                <label>Descripción (opcional)</label>
                <input value={form.descripcion} onChange={(e) => cambiarCampo('descripcion', e.target.value)} />
              </div>
              <div className="inv-campo">
                <label>Categoría</label>
                {!mostrarNuevaCategoria ? (
                  <>
                    <select value={form.categoria_id} onChange={(e) => cambiarCampo('categoria_id', e.target.value)}>
                      <option value="">Selecciona...</option>
                      {categorias.map((c) => (
                        <option key={c.id} value={c.id}>
                          {c.nombre}
                        </option>
                      ))}
                    </select>
                    <button
                      type="button"
                      className="inv-boton-nueva-categoria"
                      onClick={() => setMostrarNuevaCategoria(true)}
                    >
                      + Nueva categoría
                    </button>
                  </>
                ) : (
                  <div className="inv-nueva-categoria-fila">
                    <input
                      type="text"
                      placeholder="Nombre de la categoría"
                      value={nombreNuevaCategoria}
                      onChange={(e) => setNombreNuevaCategoria(e.target.value)}
                      autoFocus
                    />
                    <button
                      type="button"
                      className="inv-boton-guardar-categoria"
                      onClick={crearCategoriaNueva}
                      disabled={creandoCategoria}
                    >
                      {creandoCategoria ? '...' : 'Crear'}
                    </button>
                    <button
                      type="button"
                      className="inv-boton-cancelar-categoria"
                      onClick={() => {
                        setMostrarNuevaCategoria(false);
                        setNombreNuevaCategoria('');
                      }}
                    >
                      ×
                    </button>
                  </div>
                )}
              </div>
              <div className="inv-campo">
                <label>Unidad de medida</label>
                <select value={form.unidad_medida} onChange={(e) => cambiarCampo('unidad_medida', e.target.value)}>
                  {opcionesUnidad(unidadesActivas, form.unidad_medida).map((u) => (
                    <option key={u.valor} value={u.valor}>
                      {u.label}
                    </option>
                  ))}
                </select>
              </div>
              <div className="inv-campo">
                <label>Precio de venta (S/)</label>
                <input type="number" value={form.precio} onChange={(e) => cambiarCampo('precio', e.target.value)} />
              </div>
              {esAdmin && (
                <div className="inv-campo">
                  <label>IGV de este producto</label>
                  <select value={form.afectacion_igv} onChange={(e) => cambiarCampo('afectacion_igv', e.target.value)}>
                    <option value="HEREDAR">
                      Igual que su categoría (
                      {etiquetaAfectacion(categorias.find((c) => String(c.id) === form.categoria_id)?.afectacion_igv)})
                    </option>
                    {AFECTACIONES.map((a) => (
                      <option key={a.valor} value={a.valor}>
                        {a.label}
                      </option>
                    ))}
                  </select>
                </div>
              )}
              <div className="inv-campo">
                <label>{ganancias ? 'Precio de compra (S/, con IGV)' : 'Precio de compra (S/, opcional)'}</label>
                <input
                  type="number"
                  value={form.precio_compra}
                  onChange={(e) => cambiarCampo('precio_compra', e.target.value)}
                />
                {ayudaCosto && <small className={`inv-ayuda-costo${ayudaCosto.falta ? ' falta' : ''}`}>{ayudaCosto.texto}</small>}
              </div>

              {servicios && (
                <div className="inv-campo inv-campo-checkbox">
                  <label>
                    <input
                      type="checkbox"
                      checked={!form.controla_stock}
                      onChange={(e) => cambiarCampo('controla_stock', !e.target.checked)}
                    />
                    {etiquetas.sinStock || 'Es un servicio o se vende sin controlar stock (corte, instalación, delivery)'}
                  </label>
                </div>
              )}

              {!form.lleva_vencimiento && (!servicios || form.controla_stock) && (
                <div className="inv-campo">
                  <label>Stock {editandoId ? '' : 'inicial'}</label>
                  <input type="number" value={form.stock} onChange={(e) => cambiarCampo('stock', e.target.value)} />
                </div>
              )}

              {(!servicios || form.controla_stock) && (
                <>
                  <div className="inv-campo">
                    <label>Stock mínimo (alerta)</label>
                    <input
                      type="number"
                      value={form.stock_minimo}
                      onChange={(e) => cambiarCampo('stock_minimo', e.target.value)}
                    />
                  </div>

                  <div className="inv-campo inv-campo-checkbox">
                    <label>
                      <input
                        type="checkbox"
                        checked={form.lleva_vencimiento}
                        onChange={(e) => cambiarCampo('lleva_vencimiento', e.target.checked)}
                      />
                      Es perecible (maneja lotes con fecha de vencimiento)
                    </label>
                  </div>
                </>
              )}
            </div>

            {form.lleva_vencimiento && !editandoId && (!servicios || form.controla_stock) && (
              <div className="inv-lote-caja">
                <p className="inv-lote-titulo">Primer lote de este producto</p>
                <p className="inv-lote-nota">
                  El stock de productos perecibles se calcula por lotes, no se escribe a mano.
                </p>
                <div className="inv-lote-form">
                  <div className="inv-campo">
                    <label>Cantidad</label>
                    <input
                      type="number"
                      value={loteInicialCantidad}
                      onChange={(e) => setLoteInicialCantidad(e.target.value)}
                    />
                  </div>
                  <div className="inv-campo">
                    <label>Fecha de vencimiento</label>
                    <input
                      type="date"
                      value={loteInicialFecha}
                      onChange={(e) => setLoteInicialFecha(e.target.value)}
                    />
                  </div>
                </div>
              </div>
            )}

            {form.lleva_vencimiento && editandoId && (
              <div className="inv-lote-caja">
                <p className="inv-lote-titulo">Lotes de este producto</p>
                {cargandoLotes ? (
                  <p className="inv-lote-nota">Cargando lotes...</p>
                ) : (
                  <>
                    {lotesProducto.length === 0 ? (
                      <p className="inv-lote-nota">Este producto aún no tiene lotes registrados.</p>
                    ) : (
                      <div className="inv-lote-lista">
                        {lotesProducto.map((l) => (
                          <div key={l.id} className="inv-lote-item">
                            <span>{l.cantidad} unid.</span>
                            <span>Vence: {l.fecha_vencimiento}</span>
                            {l.numero_lote && <span className="inv-lote-codigo">{l.numero_lote}</span>}
                          </div>
                        ))}
                      </div>
                    )}

                    <p className="inv-lote-subtitulo">Agregar nuevo lote (ej: nueva mercadería recibida)</p>
                    <div className="inv-lote-form">
                      <div className="inv-campo">
                        <label>Cantidad</label>
                        <input
                          type="number"
                          value={nuevoLoteCantidad}
                          onChange={(e) => setNuevoLoteCantidad(e.target.value)}
                        />
                      </div>
                      <div className="inv-campo">
                        <label>Fecha de vencimiento</label>
                        <input
                          type="date"
                          value={nuevoLoteFecha}
                          onChange={(e) => setNuevoLoteFecha(e.target.value)}
                        />
                      </div>
                    </div>
                    <button
                      className="inv-boton-agregar-lote"
                      onClick={agregarLoteAProductoExistente}
                      disabled={agregandoLote}
                    >
                      {agregandoLote ? 'Agregando...' : '+ Agregar lote'}
                    </button>
                  </>
                )}
              </div>
            )}

            {mensaje && <p className={`inv-mensaje inv-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

            <div className="inv-modal-acciones">
              <button className="inv-boton-cancelar" onClick={cerrarForm}>
                Cancelar
              </button>
              <button className="inv-boton-guardar" onClick={validarYGuardar} disabled={guardando}>
                {guardando ? 'Guardando...' : editandoId ? 'Guardar cambios' : 'Crear producto'}
              </button>
            </div>
          </div>
        </div>
      )}

      {modeloForm && (
        <FormularioModelo
          grupo={modeloForm.grupo}
          ganancias={ganancias}
          categorias={categorias}
          unidadesActivas={unidadesActivas}
          onCerrar={() => setModeloForm(null)}
          onQuitarVariante={eliminarOProducto}
          onCategorias={setCategorias}
          onGuardado={(aviso) => {
            setModeloForm(null);
            setMensaje(aviso);
            cargarTodo();
          }}
        />
      )}

      {escanerCodigoAbierto && (
        <EscanerCodigoBarras
          cerrarAlDetectar
          onCodigoDetectado={(codigo) => cambiarCampo('codigo', codigo)}
          onCerrar={() => setEscanerCodigoAbierto(false)}
        />
      )}
    </div>
  );
}