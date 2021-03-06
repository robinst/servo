/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use dom::attr::AttrHelpers;
use dom::bindings::codegen::Bindings::DocumentBinding;
use dom::bindings::codegen::Bindings::DocumentBinding::DocumentMethods;
use dom::bindings::codegen::Bindings::EventHandlerBinding::EventHandlerNonNull;
use dom::bindings::codegen::Bindings::NodeBinding::NodeMethods;
use dom::bindings::codegen::Bindings::NodeFilterBinding::NodeFilter;
use dom::bindings::codegen::Bindings::WindowBinding::WindowMethods;
use dom::bindings::codegen::InheritTypes::{DocumentDerived, EventCast, HTMLElementCast};
use dom::bindings::codegen::InheritTypes::{HTMLHeadElementCast, TextCast, ElementCast};
use dom::bindings::codegen::InheritTypes::{DocumentTypeCast, HTMLHtmlElementCast, NodeCast};
use dom::bindings::codegen::InheritTypes::{EventTargetCast, HTMLAnchorElementCast};
use dom::bindings::codegen::InheritTypes::{HTMLAnchorElementDerived, HTMLAppletElementDerived};
use dom::bindings::codegen::InheritTypes::{HTMLAreaElementDerived, HTMLEmbedElementDerived};
use dom::bindings::codegen::InheritTypes::{HTMLFormElementDerived, HTMLImageElementDerived};
use dom::bindings::codegen::InheritTypes::{HTMLScriptElementDerived};
use dom::bindings::error::{ErrorResult, Fallible, NotSupported, InvalidCharacter};
use dom::bindings::error::{HierarchyRequest, NamespaceError};
use dom::bindings::global::GlobalRef;
use dom::bindings::global;
use dom::bindings::js::{MutNullableJS, JS, JSRef, Temporary, OptionalSettable, TemporaryPushable};
use dom::bindings::js::OptionalRootable;
use dom::bindings::utils::{Reflectable, Reflector, reflect_dom_object};
use dom::bindings::utils::{xml_name_type, InvalidXMLName, Name, QName};
use dom::comment::Comment;
use dom::customevent::CustomEvent;
use dom::documentfragment::DocumentFragment;
use dom::documenttype::DocumentType;
use dom::domimplementation::DOMImplementation;
use dom::element::{Element, AttributeHandlers, get_attribute_parts};
use dom::element::{HTMLHtmlElementTypeId, HTMLHeadElementTypeId, HTMLTitleElementTypeId};
use dom::element::{HTMLBodyElementTypeId, HTMLFrameSetElementTypeId};
use dom::event::Event;
use dom::eventtarget::{EventTarget, NodeTargetTypeId, EventTargetHelpers};
use dom::htmlanchorelement::HTMLAnchorElement;
use dom::htmlcollection::{HTMLCollection, CollectionFilter};
use dom::htmlelement::HTMLElement;
use dom::htmlheadelement::HTMLHeadElement;
use dom::htmlhtmlelement::HTMLHtmlElement;
use dom::htmltitleelement::HTMLTitleElement;
use dom::location::Location;
use dom::mouseevent::MouseEvent;
use dom::node::{Node, ElementNodeTypeId, DocumentNodeTypeId, NodeHelpers};
use dom::node::{CloneChildren, DoNotCloneChildren};
use dom::nodelist::NodeList;
use dom::text::Text;
use dom::processinginstruction::ProcessingInstruction;
use dom::range::Range;
use dom::treewalker::TreeWalker;
use dom::uievent::UIEvent;
use dom::window::{Window, WindowHelpers};
use html::hubbub_html_parser::build_element_from_tag;
use hubbub::hubbub::{QuirksMode, NoQuirks, LimitedQuirks, FullQuirks};
use layout_interface::{DocumentDamageLevel, ContentChangedDocumentDamage};
use servo_util::namespace;
use servo_util::str::{DOMString, split_html_space_chars};

use string_cache::Atom;
use url::Url;

use std::collections::hashmap::HashMap;
use std::ascii::StrAsciiExt;
use std::cell::{Cell, RefCell};
use std::default::Default;
use time;

#[deriving(PartialEq)]
#[jstraceable]
pub enum IsHTMLDocument {
    HTMLDocument,
    NonHTMLDocument,
}

#[jstraceable]
#[must_root]
pub struct Document {
    pub node: Node,
    reflector_: Reflector,
    pub window: JS<Window>,
    idmap: RefCell<HashMap<Atom, Vec<JS<Element>>>>,
    implementation: MutNullableJS<DOMImplementation>,
    content_type: DOMString,
    last_modified: RefCell<Option<DOMString>>,
    pub encoding_name: RefCell<DOMString>,
    pub is_html_document: bool,
    url: Url,
    quirks_mode: Cell<QuirksMode>,
    images: MutNullableJS<HTMLCollection>,
    embeds: MutNullableJS<HTMLCollection>,
    links: MutNullableJS<HTMLCollection>,
    forms: MutNullableJS<HTMLCollection>,
    scripts: MutNullableJS<HTMLCollection>,
    anchors: MutNullableJS<HTMLCollection>,
    applets: MutNullableJS<HTMLCollection>,
}

impl DocumentDerived for EventTarget {
    fn is_document(&self) -> bool {
        self.type_id == NodeTargetTypeId(DocumentNodeTypeId)
    }
}

#[jstraceable]
struct ImagesFilter;
impl CollectionFilter for ImagesFilter {
    fn filter(&self, elem: JSRef<Element>, _root: JSRef<Node>) -> bool {
        elem.is_htmlimageelement()
    }
}

#[jstraceable]
struct EmbedsFilter;
impl CollectionFilter for EmbedsFilter {
    fn filter(&self, elem: JSRef<Element>, _root: JSRef<Node>) -> bool {
        elem.is_htmlembedelement()
    }
}

#[jstraceable]
struct LinksFilter;
impl CollectionFilter for LinksFilter {
    fn filter(&self, elem: JSRef<Element>, _root: JSRef<Node>) -> bool {
        (elem.is_htmlanchorelement() || elem.is_htmlareaelement()) && elem.has_attribute("href")
    }
}

#[jstraceable]
struct FormsFilter;
impl CollectionFilter for FormsFilter {
    fn filter(&self, elem: JSRef<Element>, _root: JSRef<Node>) -> bool {
        elem.is_htmlformelement()
    }
}

#[jstraceable]
struct ScriptsFilter;
impl CollectionFilter for ScriptsFilter {
    fn filter(&self, elem: JSRef<Element>, _root: JSRef<Node>) -> bool {
        elem.is_htmlscriptelement()
    }
}

#[jstraceable]
struct AnchorsFilter;
impl CollectionFilter for AnchorsFilter {
    fn filter(&self, elem: JSRef<Element>, _root: JSRef<Node>) -> bool {
        elem.is_htmlanchorelement() && elem.has_attribute("href")
    }
}

#[jstraceable]
struct AppletsFilter;
impl CollectionFilter for AppletsFilter {
    fn filter(&self, elem: JSRef<Element>, _root: JSRef<Node>) -> bool {
        elem.is_htmlappletelement()
    }
}

pub trait DocumentHelpers<'a> {
    fn url(self) -> &'a Url;
    fn quirks_mode(self) -> QuirksMode;
    fn set_quirks_mode(self, mode: QuirksMode);
    fn set_last_modified(self, value: DOMString);
    fn set_encoding_name(self, name: DOMString);
    fn content_changed(self);
    fn damage_and_reflow(self, damage: DocumentDamageLevel);
    fn wait_until_safe_to_modify_dom(self);
    fn unregister_named_element(self, to_unregister: JSRef<Element>, id: Atom);
    fn register_named_element(self, element: JSRef<Element>, id: Atom);
    fn load_anchor_href(self, href: DOMString);
    fn find_fragment_node(self, fragid: DOMString) -> Option<Temporary<Element>>;
}

impl<'a> DocumentHelpers<'a> for JSRef<'a, Document> {
    fn url(self) -> &'a Url {
        &self.extended_deref().url
    }

    fn quirks_mode(self) -> QuirksMode {
        self.quirks_mode.get()
    }

    fn set_quirks_mode(self, mode: QuirksMode) {
        self.quirks_mode.set(mode);
    }

    fn set_last_modified(self, value: DOMString) {
        *self.last_modified.borrow_mut() = Some(value);
    }

    fn set_encoding_name(self, name: DOMString) {
        *self.encoding_name.borrow_mut() = name;
    }

    fn content_changed(self) {
        self.damage_and_reflow(ContentChangedDocumentDamage);
    }

    fn damage_and_reflow(self, damage: DocumentDamageLevel) {
        self.window.root().damage_and_reflow(damage);
    }

    fn wait_until_safe_to_modify_dom(self) {
        self.window.root().wait_until_safe_to_modify_dom();
    }


    /// Remove any existing association between the provided id and any elements in this document.
    fn unregister_named_element(self,
                                to_unregister: JSRef<Element>,
                                id: Atom) {
        let mut idmap = self.idmap.borrow_mut();
        let is_empty = match idmap.find_mut(&id) {
            None => false,
            Some(elements) => {
                let position = elements.iter()
                                       .map(|elem| elem.root())
                                       .position(|element| *element == to_unregister)
                                       .expect("This element should be in registered.");
                elements.remove(position);
                elements.is_empty()
            }
        };
        if is_empty {
            idmap.remove(&id);
        }
    }

    /// Associate an element present in this document with the provided id.
    fn register_named_element(self,
                              element: JSRef<Element>,
                              id: Atom) {
        assert!({
            let node: JSRef<Node> = NodeCast::from_ref(element);
            node.is_in_doc()
        });
        assert!(!id.as_slice().is_empty());

        let mut idmap = self.idmap.borrow_mut();

        // FIXME https://github.com/mozilla/rust/issues/13195
        //       Use mangle() when it exists again.
        let root = self.GetDocumentElement().expect("The element is in the document, so there must be a document element.").root();
        match idmap.find_mut(&id) {
            Some(elements) => {
                let new_node: JSRef<Node> = NodeCast::from_ref(element);
                let mut head : uint = 0u;
                let root: JSRef<Node> = NodeCast::from_ref(*root);
                for node in root.traverse_preorder() {
                    let elem: Option<JSRef<Element>> = ElementCast::to_ref(node);
                    match elem {
                        Some(elem) => {
                            if *(*elements)[head].root() == elem {
                                head = head + 1;
                            }
                            if new_node == node || head == elements.len() {
                                break;
                            }
                        }
                        None => {}
                    }
                }
                elements.insert_unrooted(head, &element);
                return;
            },
            None => (),
        }
        let mut elements = vec!();
        elements.push_unrooted(&element);
        idmap.insert(id, elements);
    }

    fn load_anchor_href(self, href: DOMString) {
        let window = self.window.root();
        window.load_url(href);
    }

    /// Attempt to find a named element in this page's document.
    /// https://html.spec.whatwg.org/multipage/#the-indicated-part-of-the-document
    fn find_fragment_node(self, fragid: DOMString) -> Option<Temporary<Element>> {
        self.GetElementById(fragid.clone()).or_else(|| {
            let check_anchor = |&node: &JSRef<HTMLAnchorElement>| {
                let elem: JSRef<Element> = ElementCast::from_ref(node);
                elem.get_attribute(ns!(""), "name").root().map_or(false, |attr| {
                    attr.deref().value().as_slice() == fragid.as_slice()
                })
            };
            let doc_node: JSRef<Node> = NodeCast::from_ref(self);
            doc_node.traverse_preorder()
                    .filter_map(|node| HTMLAnchorElementCast::to_ref(node))
                    .find(check_anchor)
                    .map(|node| Temporary::from_rooted(ElementCast::from_ref(node)))
        })
    }
}

impl Document {
    fn new_inherited(window: JSRef<Window>,
                         url: Option<Url>,
                         is_html_document: IsHTMLDocument,
                         content_type: Option<DOMString>) -> Document {
        let url = url.unwrap_or_else(|| Url::parse("about:blank").unwrap());

        Document {
            node: Node::new_without_doc(DocumentNodeTypeId),
            reflector_: Reflector::new(),
            window: JS::from_rooted(window),
            idmap: RefCell::new(HashMap::new()),
            implementation: Default::default(),
            content_type: match content_type {
                Some(string) => string.clone(),
                None => match is_html_document {
                    // http://dom.spec.whatwg.org/#dom-domimplementation-createhtmldocument
                    HTMLDocument => "text/html".to_string(),
                    // http://dom.spec.whatwg.org/#concept-document-content-type
                    NonHTMLDocument => "application/xml".to_string()
                }
            },
            last_modified: RefCell::new(None),
            url: url,
            // http://dom.spec.whatwg.org/#concept-document-quirks
            quirks_mode: Cell::new(NoQuirks),
            // http://dom.spec.whatwg.org/#concept-document-encoding
            encoding_name: RefCell::new("utf-8".to_string()),
            is_html_document: is_html_document == HTMLDocument,
            images: Default::default(),
            embeds: Default::default(),
            links: Default::default(),
            forms: Default::default(),
            scripts: Default::default(),
            anchors: Default::default(),
            applets: Default::default(),
        }
    }

    // http://dom.spec.whatwg.org/#dom-document
    pub fn Constructor(global: &GlobalRef) -> Fallible<Temporary<Document>> {
        Ok(Document::new(global.as_window(), None, NonHTMLDocument, None))
    }

    pub fn new(window: JSRef<Window>, url: Option<Url>, doctype: IsHTMLDocument, content_type: Option<DOMString>) -> Temporary<Document> {
        let document = reflect_dom_object(box Document::new_inherited(window, url, doctype, content_type),
                                          &global::Window(window),
                                          DocumentBinding::Wrap).root();

        let node: JSRef<Node> = NodeCast::from_ref(*document);
        node.set_owner_doc(*document);
        Temporary::from_rooted(*document)
    }
}

impl Reflectable for Document {
    fn reflector<'a>(&'a self) -> &'a Reflector {
        self.node.reflector()
    }
}

trait PrivateDocumentHelpers {
    fn createNodeList(self, callback: |node: JSRef<Node>| -> bool) -> Temporary<NodeList>;
    fn get_html_element(self) -> Option<Temporary<HTMLHtmlElement>>;
}

impl<'a> PrivateDocumentHelpers for JSRef<'a, Document> {
    fn createNodeList(self, callback: |node: JSRef<Node>| -> bool) -> Temporary<NodeList> {
        let window = self.window.root();

        match self.GetDocumentElement().root() {
            None => {
                NodeList::new_simple_list(*window, vec!())
            },
            Some(root) => {
                let mut nodes = vec!();
                let root: JSRef<Node> = NodeCast::from_ref(*root);
                for child in root.traverse_preorder() {
                    if callback(child) {
                        nodes.push(child);
                    }
                }
                NodeList::new_simple_list(*window, nodes)
            }
        }

    }

    fn get_html_element(self) -> Option<Temporary<HTMLHtmlElement>> {
        match self.GetDocumentElement().root() {
            Some(ref root) if {
                let root: JSRef<Node> = NodeCast::from_ref(**root);
                root.type_id() == ElementNodeTypeId(HTMLHtmlElementTypeId)
            } => Some(Temporary::from_rooted(HTMLHtmlElementCast::to_ref(**root).unwrap())),

            _ => None,
        }
    }
}

impl<'a> DocumentMethods for JSRef<'a, Document> {
    // http://dom.spec.whatwg.org/#dom-document-implementation
    fn Implementation(self) -> Temporary<DOMImplementation> {
        if self.implementation.get().is_none() {
            self.implementation.assign(Some(DOMImplementation::new(self)));
        }
        self.implementation.get().unwrap()
    }

    // http://dom.spec.whatwg.org/#dom-document-url
    fn URL(self) -> DOMString {
        self.url().serialize()
    }

    // http://dom.spec.whatwg.org/#dom-document-documenturi
    fn DocumentURI(self) -> DOMString {
        self.URL()
    }

    // http://dom.spec.whatwg.org/#dom-document-compatmode
    fn CompatMode(self) -> DOMString {
        match self.quirks_mode.get() {
            LimitedQuirks | NoQuirks => "CSS1Compat".to_string(),
            FullQuirks => "BackCompat".to_string()
        }
    }

    // http://dom.spec.whatwg.org/#dom-document-characterset
    fn CharacterSet(self) -> DOMString {
        self.encoding_name.borrow().as_slice().to_ascii_lower()
    }

    // http://dom.spec.whatwg.org/#dom-document-content_type
    fn ContentType(self) -> DOMString {
        self.content_type.clone()
    }

    // http://dom.spec.whatwg.org/#dom-document-doctype
    fn GetDoctype(self) -> Option<Temporary<DocumentType>> {
        let node: JSRef<Node> = NodeCast::from_ref(self);
        node.children().find(|child| {
            child.is_doctype()
        }).map(|node| {
            let doctype: JSRef<DocumentType> = DocumentTypeCast::to_ref(node).unwrap();
            Temporary::from_rooted(doctype)
        })
    }

    // http://dom.spec.whatwg.org/#dom-document-documentelement
    fn GetDocumentElement(self) -> Option<Temporary<Element>> {
        let node: JSRef<Node> = NodeCast::from_ref(self);
        node.child_elements().next().map(|elem| Temporary::from_rooted(elem))
    }

    // http://dom.spec.whatwg.org/#dom-document-getelementsbytagname
    fn GetElementsByTagName(self, tag_name: DOMString) -> Temporary<HTMLCollection> {
        let window = self.window.root();
        HTMLCollection::by_tag_name(*window, NodeCast::from_ref(self), tag_name)
    }

    // http://dom.spec.whatwg.org/#dom-document-getelementsbytagnamens
    fn GetElementsByTagNameNS(self, maybe_ns: Option<DOMString>, tag_name: DOMString) -> Temporary<HTMLCollection> {
        let window = self.window.root();
        HTMLCollection::by_tag_name_ns(*window, NodeCast::from_ref(self), tag_name, maybe_ns)
    }

    // http://dom.spec.whatwg.org/#dom-document-getelementsbyclassname
    fn GetElementsByClassName(self, classes: DOMString) -> Temporary<HTMLCollection> {
        let window = self.window.root();

        HTMLCollection::by_class_name(*window, NodeCast::from_ref(self), classes)
    }

    // http://dom.spec.whatwg.org/#dom-nonelementparentnode-getelementbyid
    fn GetElementById(self, id: DOMString) -> Option<Temporary<Element>> {
        let id = Atom::from_slice(id.as_slice());
        match self.idmap.borrow().find(&id) {
            None => None,
            Some(ref elements) => Some(Temporary::new((*elements)[0].clone())),
        }
    }

    // http://dom.spec.whatwg.org/#dom-document-createelement
    fn CreateElement(self, local_name: DOMString) -> Fallible<Temporary<Element>> {
        if xml_name_type(local_name.as_slice()) == InvalidXMLName {
            debug!("Not a valid element name");
            return Err(InvalidCharacter);
        }
        let local_name = local_name.as_slice().to_ascii_lower();
        Ok(build_element_from_tag(local_name, ns!(HTML), None, self))
    }

    // http://dom.spec.whatwg.org/#dom-document-createelementns
    fn CreateElementNS(self,
                       namespace: Option<DOMString>,
                       qualified_name: DOMString) -> Fallible<Temporary<Element>> {
        let ns = namespace::from_domstring(namespace);
        match xml_name_type(qualified_name.as_slice()) {
            InvalidXMLName => {
                debug!("Not a valid element name");
                return Err(InvalidCharacter);
            },
            Name => {
                debug!("Not a valid qualified element name");
                return Err(NamespaceError);
            },
            QName => {}
        }

        let (prefix_from_qname,
             local_name_from_qname) = get_attribute_parts(qualified_name.as_slice());
        match (&ns, prefix_from_qname.clone(), local_name_from_qname.as_slice()) {
            // throw if prefix is not null and namespace is null
            (&ns!(""), Some(_), _) => {
                debug!("Namespace can't be null with a non-null prefix");
                return Err(NamespaceError);
            },
            // throw if prefix is "xml" and namespace is not the XML namespace
            (_, Some(ref prefix), _) if "xml" == prefix.as_slice() && ns != ns!(XML) => {
                debug!("Namespace must be the xml namespace if the prefix is 'xml'");
                return Err(NamespaceError);
            },
            // throw if namespace is the XMLNS namespace and neither qualifiedName nor prefix is "xmlns"
            (&ns!(XMLNS), Some(ref prefix), _) if "xmlns" == prefix.as_slice() => {},
            (&ns!(XMLNS), _, "xmlns") => {},
            (&ns!(XMLNS), _, _) => {
                debug!("The prefix or the qualified name must be 'xmlns' if namespace is the XMLNS namespace ");
                return Err(NamespaceError);
            },
            _ => {}
        }

        if ns == ns!(HTML) {
            Ok(build_element_from_tag(local_name_from_qname.to_string(), ns,
                                      prefix_from_qname.map(|s| s.to_string()), self))
        } else {
            Ok(Element::new(local_name_from_qname.to_string(), ns,
                            prefix_from_qname.map(|s| s.to_string()), self))
        }
    }

    // http://dom.spec.whatwg.org/#dom-document-createdocumentfragment
    fn CreateDocumentFragment(self) -> Temporary<DocumentFragment> {
        DocumentFragment::new(self)
    }

    // http://dom.spec.whatwg.org/#dom-document-createtextnode
    fn CreateTextNode(self, data: DOMString)
                          -> Temporary<Text> {
        Text::new(data, self)
    }

    // http://dom.spec.whatwg.org/#dom-document-createcomment
    fn CreateComment(self, data: DOMString) -> Temporary<Comment> {
        Comment::new(data, self)
    }

    // http://dom.spec.whatwg.org/#dom-document-createprocessinginstruction
    fn CreateProcessingInstruction(self, target: DOMString,
                                       data: DOMString) -> Fallible<Temporary<ProcessingInstruction>> {
        // Step 1.
        if xml_name_type(target.as_slice()) == InvalidXMLName {
            return Err(InvalidCharacter);
        }

        // Step 2.
        if data.as_slice().contains("?>") {
            return Err(InvalidCharacter);
        }

        // Step 3.
        Ok(ProcessingInstruction::new(target, data, self))
    }

    // http://dom.spec.whatwg.org/#dom-document-importnode
    fn ImportNode(self, node: JSRef<Node>, deep: bool) -> Fallible<Temporary<Node>> {
        // Step 1.
        if node.is_document() {
            return Err(NotSupported);
        }

        // Step 2.
        let clone_children = match deep {
            true => CloneChildren,
            false => DoNotCloneChildren
        };

        Ok(Node::clone(node, Some(self), clone_children))
    }

    // http://dom.spec.whatwg.org/#dom-document-adoptnode
    fn AdoptNode(self, node: JSRef<Node>) -> Fallible<Temporary<Node>> {
        // Step 1.
        if node.is_document() {
            return Err(NotSupported);
        }

        // Step 2.
        Node::adopt(node, self);

        // Step 3.
        Ok(Temporary::from_rooted(node))
    }

    // http://dom.spec.whatwg.org/#dom-document-createevent
    fn CreateEvent(self, interface: DOMString) -> Fallible<Temporary<Event>> {
        let window = self.window.root();

        match interface.as_slice().to_ascii_lower().as_slice() {
            "uievents" | "uievent" => Ok(EventCast::from_temporary(
                UIEvent::new_uninitialized(*window))),
            "mouseevents" | "mouseevent" => Ok(EventCast::from_temporary(
                MouseEvent::new_uninitialized(*window))),
            "customevent" => Ok(EventCast::from_temporary(
                CustomEvent::new_uninitialized(&global::Window(*window)))),
            "htmlevents" | "events" | "event" => Ok(Event::new_uninitialized(
                &global::Window(*window))),
            _ => Err(NotSupported)
        }
    }

    // http://www.whatwg.org/html/#dom-document-lastmodified
    fn LastModified(self) -> DOMString {
        match *self.last_modified.borrow() {
            Some(ref t) => t.clone(),
            None => time::now().strftime("%m/%d/%Y %H:%M:%S"),
        }
    }

    // http://dom.spec.whatwg.org/#dom-document-createrange
    fn CreateRange(self) -> Temporary<Range> {
        Range::new(self)
    }

    // http://dom.spec.whatwg.org/#dom-document-createtreewalker
    fn CreateTreeWalker(self, root: JSRef<Node>, whatToShow: u32, filter: Option<NodeFilter>)
                        -> Temporary<TreeWalker> {
        TreeWalker::new(self, root, whatToShow, filter)
    }

    // http://www.whatwg.org/specs/web-apps/current-work/#document.title
    fn Title(self) -> DOMString {
        let mut title = String::new();
        self.GetDocumentElement().root().map(|root| {
            let root: JSRef<Node> = NodeCast::from_ref(*root);
            root.traverse_preorder()
                .find(|node| node.type_id() == ElementNodeTypeId(HTMLTitleElementTypeId))
                .map(|title_elem| {
                    for child in title_elem.children() {
                        if child.is_text() {
                            let text: JSRef<Text> = TextCast::to_ref(child).unwrap();
                            title.push_str(text.deref().characterdata.data.borrow().as_slice());
                        }
                    }
                });
        });
        let v: Vec<&str> = split_html_space_chars(title.as_slice()).collect();
        v.connect(" ")
    }

    // http://www.whatwg.org/specs/web-apps/current-work/#document.title
    fn SetTitle(self, title: DOMString) -> ErrorResult {
        self.GetDocumentElement().root().map(|root| {
            let root: JSRef<Node> = NodeCast::from_ref(*root);
            let head_node = root.traverse_preorder().find(|child| {
                child.type_id() == ElementNodeTypeId(HTMLHeadElementTypeId)
            });
            head_node.map(|head| {
                let title_node = head.children().find(|child| {
                    child.type_id() == ElementNodeTypeId(HTMLTitleElementTypeId)
                });

                match title_node {
                    Some(ref title_node) => {
                        for title_child in title_node.children() {
                            assert!(title_node.RemoveChild(title_child).is_ok());
                        }
                        if !title.is_empty() {
                            let new_text = self.CreateTextNode(title.clone()).root();
                            assert!(title_node.AppendChild(NodeCast::from_ref(*new_text)).is_ok());
                        }
                    },
                    None => {
                        let new_title = HTMLTitleElement::new("title".to_string(), None, self).root();
                        let new_title: JSRef<Node> = NodeCast::from_ref(*new_title);

                        if !title.is_empty() {
                            let new_text = self.CreateTextNode(title.clone()).root();
                            assert!(new_title.AppendChild(NodeCast::from_ref(*new_text)).is_ok());
                        }
                        assert!(head.AppendChild(new_title).is_ok());
                    },
                }
            });
        });
        Ok(())
    }

    // http://www.whatwg.org/specs/web-apps/current-work/#dom-document-head
    fn GetHead(self) -> Option<Temporary<HTMLHeadElement>> {
        self.get_html_element().and_then(|root| {
            let root = root.root();
            let node: JSRef<Node> = NodeCast::from_ref(*root);
            node.children().find(|child| {
                child.type_id() == ElementNodeTypeId(HTMLHeadElementTypeId)
            }).map(|node| {
                Temporary::from_rooted(HTMLHeadElementCast::to_ref(node).unwrap())
            })
        })
    }

    // http://www.whatwg.org/specs/web-apps/current-work/#dom-document-body
    fn GetBody(self) -> Option<Temporary<HTMLElement>> {
        self.get_html_element().and_then(|root| {
            let root = root.root();
            let node: JSRef<Node> = NodeCast::from_ref(*root);
            node.children().find(|child| {
                match child.type_id() {
                    ElementNodeTypeId(HTMLBodyElementTypeId) |
                    ElementNodeTypeId(HTMLFrameSetElementTypeId) => true,
                    _ => false
                }
            }).map(|node| {
                Temporary::from_rooted(HTMLElementCast::to_ref(node).unwrap())
            })
        })
    }

    // http://www.whatwg.org/specs/web-apps/current-work/#dom-document-body
    fn SetBody(self, new_body: Option<JSRef<HTMLElement>>) -> ErrorResult {
        // Step 1.
        match new_body {
            Some(ref htmlelem) => {
                let node: JSRef<Node> = NodeCast::from_ref(*htmlelem);
                match node.type_id() {
                    ElementNodeTypeId(HTMLBodyElementTypeId) | ElementNodeTypeId(HTMLFrameSetElementTypeId) => {}
                    _ => return Err(HierarchyRequest)
                }
            }
            None => return Err(HierarchyRequest)
        }

        // Step 2.
        let old_body = self.GetBody().root();
        //FIXME: covariant lifetime workaround. do not judge.
        if old_body.as_ref().map(|body| body.deref()) == new_body.as_ref().map(|a| &*a) {
            return Ok(());
        }

        // Step 3.
        match self.get_html_element().root() {
            // Step 4.
            None => return Err(HierarchyRequest),
            Some(ref root) => {
                let new_body_unwrapped = new_body.unwrap();
                let new_body: JSRef<Node> = NodeCast::from_ref(new_body_unwrapped);

                let root: JSRef<Node> = NodeCast::from_ref(**root);
                match old_body {
                    Some(ref child) => {
                        let child: JSRef<Node> = NodeCast::from_ref(**child);

                        assert!(root.ReplaceChild(new_body, child).is_ok())
                    }
                    None => assert!(root.AppendChild(new_body).is_ok())
                };
            }
        }
        Ok(())
    }

    // http://www.whatwg.org/specs/web-apps/current-work/#dom-document-getelementsbyname
    fn GetElementsByName(self, name: DOMString) -> Temporary<NodeList> {
        self.createNodeList(|node| {
            if !node.is_element() {
                return false;
            }

            let element: JSRef<Element> = ElementCast::to_ref(node).unwrap();
            element.get_attribute(ns!(""), "name").root().map_or(false, |attr| {
                attr.value().as_slice() == name.as_slice()
            })
        })
    }

    fn Images(self) -> Temporary<HTMLCollection> {
        if self.images.get().is_none() {
            let window = self.window.root();
            let root = NodeCast::from_ref(self);
            let filter = box ImagesFilter;
            self.images.assign(Some(HTMLCollection::create(*window, root, filter)));
        }
        self.images.get().unwrap()
    }

    fn Embeds(self) -> Temporary<HTMLCollection> {
        if self.embeds.get().is_none() {
            let window = self.window.root();
            let root = NodeCast::from_ref(self);
            let filter = box EmbedsFilter;
            self.embeds.assign(Some(HTMLCollection::create(*window, root, filter)));
        }
        self.embeds.get().unwrap()
    }

    fn Plugins(self) -> Temporary<HTMLCollection> {
        self.Embeds()
    }

    fn Links(self) -> Temporary<HTMLCollection> {
        if self.links.get().is_none() {
            let window = self.window.root();
            let root = NodeCast::from_ref(self);
            let filter = box LinksFilter;
            self.links.assign(Some(HTMLCollection::create(*window, root, filter)));
        }
        self.links.get().unwrap()
    }

    fn Forms(self) -> Temporary<HTMLCollection> {
        if self.forms.get().is_none() {
            let window = self.window.root();
            let root = NodeCast::from_ref(self);
            let filter = box FormsFilter;
            self.forms.assign(Some(HTMLCollection::create(*window, root, filter)));
        }
        self.forms.get().unwrap()
    }

    fn Scripts(self) -> Temporary<HTMLCollection> {
        if self.scripts.get().is_none() {
            let window = self.window.root();
            let root = NodeCast::from_ref(self);
            let filter = box ScriptsFilter;
            self.scripts.assign(Some(HTMLCollection::create(*window, root, filter)));
        }
        self.scripts.get().unwrap()
    }

    fn Anchors(self) -> Temporary<HTMLCollection> {
        if self.anchors.get().is_none() {
            let window = self.window.root();
            let root = NodeCast::from_ref(self);
            let filter = box AnchorsFilter;
            self.anchors.assign(Some(HTMLCollection::create(*window, root, filter)));
        }
        self.anchors.get().unwrap()
    }

    fn Applets(self) -> Temporary<HTMLCollection> {
        // FIXME: This should be return OBJECT elements containing applets.
        if self.applets.get().is_none() {
            let window = self.window.root();
            let root = NodeCast::from_ref(self);
            let filter = box AppletsFilter;
            self.applets.assign(Some(HTMLCollection::create(*window, root, filter)));
        }
        self.applets.get().unwrap()
    }

    fn Location(self) -> Temporary<Location> {
        let window = self.window.root();
        window.Location()
    }

    // http://dom.spec.whatwg.org/#dom-parentnode-children
    fn Children(self) -> Temporary<HTMLCollection> {
        let window = self.window.root();
        HTMLCollection::children(*window, NodeCast::from_ref(self))
    }

    // http://dom.spec.whatwg.org/#dom-parentnode-queryselector
    fn QuerySelector(self, selectors: DOMString) -> Fallible<Option<Temporary<Element>>> {
        let root: JSRef<Node> = NodeCast::from_ref(self);
        root.query_selector(selectors)
    }

    // http://dom.spec.whatwg.org/#dom-parentnode-queryselectorall
    fn QuerySelectorAll(self, selectors: DOMString) -> Fallible<Temporary<NodeList>> {
        let root: JSRef<Node> = NodeCast::from_ref(self);
        root.query_selector_all(selectors)
    }

    fn GetOnclick(self) -> Option<EventHandlerNonNull> {
        let eventtarget: JSRef<EventTarget> = EventTargetCast::from_ref(self);
        eventtarget.get_event_handler_common("click")
    }

    fn SetOnclick(self, listener: Option<EventHandlerNonNull>) {
        let eventtarget: JSRef<EventTarget> = EventTargetCast::from_ref(self);
        eventtarget.set_event_handler_common("click", listener)
    }

    fn GetOnload(self) -> Option<EventHandlerNonNull> {
        let eventtarget: JSRef<EventTarget> = EventTargetCast::from_ref(self);
        eventtarget.get_event_handler_common("load")
    }

    fn SetOnload(self, listener: Option<EventHandlerNonNull>) {
        let eventtarget: JSRef<EventTarget> = EventTargetCast::from_ref(self);
        eventtarget.set_event_handler_common("load", listener)
    }
}
